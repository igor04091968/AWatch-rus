CREATE TABLE IF NOT EXISTS aw_workforce.dim_workstation_user
(
    host_name String,
    user_login String,
    user_domain String,

    employee_id String,
    employee_name String,
    department String,
    branch String,
    position String,

    source LowCardinality(String),
    is_active UInt8 DEFAULT 1,
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY (host_name, user_login);

CREATE TABLE IF NOT EXISTS aw_workforce.dim_application_category
(
    process_name String,

    application_name String,
    vendor String,
    category LowCardinality(String),
    productivity_class LowCardinality(String),
    risk_level LowCardinality(String),

    is_system UInt8 DEFAULT 0,
    is_active UInt8 DEFAULT 1,
    source LowCardinality(String),
    comment String,
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY process_name;

CREATE TABLE IF NOT EXISTS aw_workforce.dim_domain_category
(
    domain String,

    site_name String,
    category LowCardinality(String),
    productivity_class LowCardinality(String),
    risk_level LowCardinality(String),
    business_allowed UInt8 DEFAULT 0,

    source LowCardinality(String),
    comment String,
    is_active UInt8 DEFAULT 1,
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY domain;

CREATE TABLE IF NOT EXISTS aw_workforce.dim_url_rule
(
    rule_id String,
    domain String,
    path_pattern String,

    category LowCardinality(String),
    productivity_class LowCardinality(String),
    risk_level LowCardinality(String),

    priority UInt16 DEFAULT 100,
    is_active UInt8 DEFAULT 1,
    comment String,
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY (domain, priority, rule_id);

DROP DICTIONARY IF EXISTS aw_workforce.dict_workstation_user;

CREATE DICTIONARY aw_workforce.dict_workstation_user
(
    host_name String,
    user_login String,
    user_domain String,

    employee_id String,
    employee_name String,
    department String,
    branch String,
    position String,
    is_active UInt8
)
PRIMARY KEY host_name, user_login
SOURCE(CLICKHOUSE(
    USER 'aw_workforce_dict'
    PASSWORD ''
    DB 'aw_workforce'
    TABLE 'dim_workstation_user'
))
LAYOUT(COMPLEX_KEY_HASHED())
LIFETIME(MIN 3600 MAX 86400);

DROP DICTIONARY IF EXISTS aw_workforce.dict_application_category;

CREATE DICTIONARY aw_workforce.dict_application_category
(
    process_name String,

    application_name String,
    vendor String,
    category String,
    productivity_class String,
    risk_level String,
    is_system UInt8,
    is_active UInt8
)
PRIMARY KEY process_name
SOURCE(CLICKHOUSE(
    USER 'aw_workforce_dict'
    PASSWORD ''
    DB 'aw_workforce'
    TABLE 'dim_application_category'
))
LAYOUT(HASHED())
LIFETIME(MIN 3600 MAX 86400);

DROP DICTIONARY IF EXISTS aw_workforce.dict_domain_category;

CREATE DICTIONARY aw_workforce.dict_domain_category
(
    domain String,

    site_name String,
    category String,
    productivity_class String,
    risk_level String,
    business_allowed UInt8,
    is_active UInt8
)
PRIMARY KEY domain
SOURCE(CLICKHOUSE(
    USER 'aw_workforce_dict'
    PASSWORD ''
    DB 'aw_workforce'
    TABLE 'dim_domain_category'
))
LAYOUT(HASHED())
LIFETIME(MIN 3600 MAX 86400);
