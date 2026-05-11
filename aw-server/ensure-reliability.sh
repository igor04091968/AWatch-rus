#!/bin/bash
set -euo pipefail

# Service reliability script for AW services
# Fixes common issues and ensures proper configuration

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="/etc/activitywatch/aw-server.env"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

check_env_file() {
    if [[ ! -f "$ENV_FILE" ]]; then
        log "ERROR: Environment file not found: $ENV_FILE"
        return 1
    fi
    log "✓ Environment file exists"
}

fix_permissions() {
    log "Fixing permissions..."
    
    # Ensure proper ownership
    chown -R activitywatch:activitywatch /var/lib/activitywatch
    chown -R activitywatch:activitywatch /var/log/activitywatch
    chown -R activitywatch:activitywatch /opt/activitywatch
    
    # Ensure proper permissions
    chmod 755 /var/lib/activitywatch
    chmod 755 /var/log/activitywatch
    chmod 755 /opt/activitywatch
    
    log "✓ Permissions fixed"
}

restart_services() {
    log "Restarting services..."
    
    systemctl daemon-reload
    
    # Stop all services
    systemctl stop aw-worktime-api aw-worktime-ui-bridge activitywatch-server || true
    
    # Wait for stop
    sleep 2
    
    # Start in dependency order
    systemctl start activitywatch-server
    sleep 3
    systemctl start aw-worktime-api
    sleep 2
    systemctl start aw-worktime-ui-bridge
    
    log "✓ Services restarted"
}

enable_services() {
    log "Enabling services..."
    
    systemctl enable activitywatch-server
    systemctl enable aw-worktime-api
    systemctl enable aw-worktime-ui-bridge
    
    log "✓ Services enabled"
}

setup_logrotate() {
    local logrotate_file="/etc/logrotate.d/activitywatch"
    
    if [[ ! -f "$logrotate_file" ]]; then
        log "Setting up log rotation..."
        cp "$SCRIPT_DIR/logrotate.conf" "$logrotate_file"
        log "✓ Log rotation configured"
    else
        log "✓ Log rotation already configured"
    fi
}

setup_health_check() {
    local health_script="/usr/local/bin/aw-health-check"
    local health_timer="/etc/systemd/system/aw-health-check.timer"
    local health_service="/etc/systemd/system/aw-health-check.service"
    
    if [[ ! -f "$health_script" ]]; then
        log "Setting up health check..."
        cp "$SCRIPT_DIR/health-check.sh" "$health_script"
        chmod +x "$health_script"
        
        # Create systemd timer for health checks
        cat > "$health_timer" << 'EOF'
[Unit]
Description=AW Health Check Timer
Requires=aw-health-check.service

[Timer]
OnCalendar=*:0/5:00
Persistent=true

[Install]
WantedBy=timers.target
EOF
        
        cat > "$health_service" << 'EOF'
[Unit]
Description=AW Health Check
After=network.target

[Service]
Type=oneshot
ExecStart=/usr/local/bin/aw-health-check
User=root
Group=root
EOF
        
        systemctl daemon-reload
        systemctl enable aw-health-check.timer
        systemctl start aw-health-check.timer
        
        log "✓ Health check configured"
    else
        log "✓ Health check already configured"
    fi
}

main() {
    log "=== AW Service Reliability Fix ==="
    
    check_env_file || exit 1
    fix_permissions
    setup_logrotate
    setup_health_check
    restart_services
    enable_services
    
    log
    log "=== Reliability Fix Complete ==="
    log "Check status with: systemctl status activitywatch-server aw-worktime-api aw-worktime-ui-bridge"
    log "Check health with: /usr/local/bin/aw-health-check"
    log "View logs with: journalctl -u activitywatch-server -u aw-worktime-api -u aw-worktime-ui-bridge -f"
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi