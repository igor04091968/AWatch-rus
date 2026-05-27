<?php

require_once('/etc/inc/config.inc');
require_once('/etc/inc/config.lib.inc');
require_once('/etc/inc/certs.inc');
require_once('/etc/inc/openvpn.inc');
require_once('/usr/local/pkg/openvpn-client-export.inc');

function fail_with(string $message, int $code = 1): void {
	fwrite(STDERR, $message . PHP_EOL);
	exit($code);
}

function parse_option(array &$argv, string $prefix): ?string {
	foreach ($argv as $index => $arg) {
		if (str_starts_with($arg, $prefix . '=')) {
			$value = substr($arg, strlen($prefix) + 1);
			unset($argv[$index]);
			$argv = array_values($argv);
			return $value;
		}
		if ($arg === $prefix && isset($argv[$index + 1])) {
			$value = $argv[$index + 1];
			unset($argv[$index], $argv[$index + 1]);
			$argv = array_values($argv);
			return $value;
		}
	}
	return null;
}

function normalize_common_name(string $raw): string {
	$value = trim($raw);
	$value = preg_replace('/[^A-Za-z0-9_.-]/', '_', $value);
	$value = trim($value, '._-');
	if ($value === '') {
		fail_with('common name is empty after normalization');
	}
	return $value;
}

function build_cert_index_by_descr(array $certs): array {
	$out = [];
	foreach ($certs as $index => $cert) {
		$descr = trim((string)($cert['descr'] ?? ''));
		if ($descr !== '') {
			$out[$descr] = $index;
		}
	}
	return $out;
}

function build_csc_index_by_common_name(array $entries): array {
	$out = [];
	foreach ($entries as $index => $entry) {
		$commonName = trim((string)($entry['common_name'] ?? ''));
		if ($commonName !== '') {
			$out[$commonName] = $index;
		}
	}
	return $out;
}

function default_csc_template(): array {
	return [
		'custom_options' => '',
		'local_network' => '10.0.0.0/8, 172.16.0.0/12, 192.168.100.0/24',
		'local_networkv6' => '',
		'remote_network' => '',
		'remote_networkv6' => '',
		'dns_server1' => '192.168.100.1',
		'dns_server2' => '192.168.100.250',
		'dns_server3' => '',
		'dns_server4' => '',
		'push_blockoutsidedns' => '',
		'push_register_dns' => '',
		'netbios_enable' => 'yes',
		'netbios_ntype' => '0',
		'netbios_scope' => '',
	];
}

function pick_csc_template(array $entries, string $serverId): array {
	$fields = array_keys(default_csc_template());
	$variants = [];
	$counts = [];

	foreach ($entries as $entry) {
		if ((string)($entry['server_list'] ?? '') !== $serverId) {
			continue;
		}
		$variant = [];
		foreach ($fields as $field) {
			$variant[$field] = (string)($entry[$field] ?? '');
		}
		$key = json_encode($variant, JSON_UNESCAPED_SLASHES);
		$counts[$key] = ($counts[$key] ?? 0) + 1;
		$variants[$key] = $variant;
	}

	if ($counts === []) {
		return default_csc_template();
	}

	arsort($counts, SORT_NUMERIC);
	$bestKey = array_key_first($counts);
	return $variants[$bestKey] ?? default_csc_template();
}

function allocate_tunnel_network(array $server, array $entries, string $serverId, int $firstHostOffset): string {
	$tunnelNetwork = trim((string)($server['tunnel_network'] ?? ''));
	if (!preg_match('/^([0-9.]+)\/(\d{1,2})$/', $tunnelNetwork, $match)) {
		fail_with("unsupported tunnel network: {$tunnelNetwork}");
	}

	$networkIp = $match[1];
	$prefix = (int)$match[2];
	if ($prefix < 1 || $prefix > 30) {
		fail_with("unsupported tunnel prefix: {$prefix}");
	}

	$networkLong = ip2long($networkIp);
	if ($networkLong === false) {
		fail_with("invalid tunnel network IP: {$networkIp}");
	}

	$blockSize = 1 << (32 - $prefix);
	$broadcastLong = $networkLong + $blockSize - 1;
	$startLong = $networkLong + max(1, $firstHostOffset);

	$used = [];
	foreach ($entries as $entry) {
		if ((string)($entry['server_list'] ?? '') !== $serverId) {
			continue;
		}
		$value = trim((string)($entry['tunnel_network'] ?? ''));
		if (!preg_match('/^([0-9.]+)\/\d+$/', $value, $entryMatch)) {
			continue;
		}
		$ipLong = ip2long($entryMatch[1]);
		if ($ipLong !== false) {
			$used[$ipLong] = true;
		}
	}

	for ($candidate = $startLong; $candidate < $broadcastLong; $candidate++) {
		if (!isset($used[$candidate])) {
			return long2ip($candidate) . '/' . $prefix;
		}
	}

	for ($candidate = $networkLong + 1; $candidate < $startLong; $candidate++) {
		if (!isset($used[$candidate])) {
			return long2ip($candidate) . '/' . $prefix;
		}
	}

	fail_with("no free client address left in {$tunnelNetwork}");
}

$argvCopy = $argv;
array_shift($argvCopy);

$commonNameRaw = parse_option($argvCopy, '--common-name');
$serverId = (int)(parse_option($argvCopy, '--server-id') ?? 1);
$firstHostOffset = (int)(parse_option($argvCopy, '--first-host-offset') ?? 13);

if ($commonNameRaw === null || trim($commonNameRaw) === '') {
	fail_with('missing --common-name');
}
if ($serverId <= 0) {
	fail_with('server id must be positive');
}

$commonName = normalize_common_name($commonNameRaw);
$server = get_openvpnserver_by_id($serverId);
if (empty($server)) {
	fail_with("OpenVPN server {$serverId} not found");
}

$caref = trim((string)($server['caref'] ?? ''));
if ($caref === '') {
	fail_with("OpenVPN server {$serverId} does not have caref");
}

$certs = config_get_path('cert', []);
$cscEntries = config_get_path('openvpn/openvpn-csc', []);
$certIndexByDescr = build_cert_index_by_descr($certs);
$cscIndexByCommonName = build_csc_index_by_common_name($cscEntries);

$certCreated = false;
$cscCreated = false;
$changed = false;
$serverKey = (string)$serverId;

if (!array_key_exists($commonName, $certIndexByDescr)) {
	$cert = [
		'refid' => uniqid('', true),
		'descr' => $commonName,
	];
	$dn = [
		'countryName' => 'RU',
		'stateOrProvinceName' => 'Komi Republic',
		'localityName' => 'Syktyvkar',
		'commonName' => $commonName,
	];
	if (!cert_create($cert, $caref, 2048, 3650, $dn, 'user', 'sha256', 'RSA')) {
		fail_with("failed to create certificate for {$commonName}");
	}
	$certs[] = $cert;
	$certIndexByDescr[$commonName] = count($certs) - 1;
	$certCreated = true;
	$changed = true;
}

if (!array_key_exists($commonName, $cscIndexByCommonName)) {
	$template = pick_csc_template($cscEntries, $serverKey);
	$template['server_list'] = $serverKey;
	$template['common_name'] = $commonName;
	$template['description'] = "{$commonName} access";
	$template['tunnel_network'] = allocate_tunnel_network($server, $cscEntries, $serverKey, $firstHostOffset);
	$template['tunnel_networkv6'] = '';
	$template['gateway'] = '';
	$template['gateway6'] = '';
	$cscEntries[] = $template;
	$cscIndexByCommonName[$commonName] = count($cscEntries) - 1;
	$cscCreated = true;
	$changed = true;
}

if ($changed) {
	config_set_path('cert', $certs);
	config_set_path('openvpn/openvpn-csc', $cscEntries);
	write_config("TSJ Guardian OpenVPN export for {$commonName}");
	$server = get_openvpnserver_by_id($serverId);
	openvpn_resync('server', $server);
	openvpn_resync_csc_all();

	$certs = config_get_path('cert', []);
	$cscEntries = config_get_path('openvpn/openvpn-csc', []);
	$certIndexByDescr = build_cert_index_by_descr($certs);
	$cscIndexByCommonName = build_csc_index_by_common_name($cscEntries);
}

$certIndex = $certIndexByDescr[$commonName] ?? null;
if ($certIndex === null) {
	fail_with("certificate index missing for {$commonName}");
}

$cscIndex = $cscIndexByCommonName[$commonName] ?? null;
if ($cscIndex === null) {
	fail_with("CSC entry missing for {$commonName}");
}

$prefix = openvpn_client_export_prefix($serverId, null, $certIndex);
if (!$prefix) {
	fail_with("failed to build export prefix for {$commonName}");
}

$inline = openvpn_client_export_config(
	$serverId,
	null,
	$certIndex,
	'serveraddr',
	'auto',
	true,
	true,
	'',
	false,
	false,
	'',
	'inline',
	'',
	'high',
	false,
	false,
	'',
	false,
	'',
	''
);
if ($inline === false || trim((string)$inline) === '') {
	fail_with("inline export failed for {$commonName}");
}

$csc = $cscEntries[$cscIndex] ?? [];
$result = [
	'common_name' => $commonName,
	'server_id' => $serverId,
	'filename' => "{$prefix}-config.ovpn",
	'tunnel_network' => (string)($csc['tunnel_network'] ?? ''),
	'cert_created' => $certCreated,
	'csc_created' => $cscCreated,
	'config_sha256' => hash('sha256', $inline),
	'config_b64' => base64_encode($inline),
];

echo json_encode($result, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES) . PHP_EOL;
