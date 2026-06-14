//! Portal role model and access-scope contract.
//!
//! CONTRACT: role aliases, serialized values and allowed scopes are part of
//! the portal API/security boundary. Keep changes explicit and covered by
//! existing role-gate tests in `main.rs`.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortalRole {
    Executive,
    Manager,
    Security,
    Forensics,
    Admin,
}

impl PortalRole {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "executive" | "owner" | "rukovoditel" | "руководитель" => {
                Some(Self::Executive)
            }
            "manager" | "workforce" | "руководитель_подразделения" => {
                Some(Self::Manager)
            }
            "security" | "ib" | "soc" | "безопасность" => Some(Self::Security),
            "forensics" | "investigation" | "расследования" => Some(Self::Forensics),
            "admin" | "operations" | "operator" | "эксплуатация" => Some(Self::Admin),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Executive => "executive",
            Self::Manager => "manager",
            Self::Security => "security",
            Self::Forensics => "forensics",
            Self::Admin => "admin",
        }
    }

    pub(crate) fn label_ru(self) -> &'static str {
        match self {
            Self::Executive => "Руководитель",
            Self::Manager => "Руководитель подразделения",
            Self::Security => "Безопасность",
            Self::Forensics => "Расследования",
            Self::Admin => "Администратор",
        }
    }

    pub(crate) fn allowed_scopes(self) -> &'static [&'static str] {
        match self {
            Self::Executive => &["executive", "workforce"],
            Self::Manager => &["executive", "workforce"],
            Self::Security => &["security", "incidents", "ueba", "pfsense"],
            Self::Forensics => &["forensics", "incidents", "ueba"],
            Self::Admin => &[
                "executive",
                "workforce",
                "security",
                "forensics",
                "incidents",
                "ueba",
                "pfsense",
                "admin",
            ],
        }
    }

    pub(crate) fn can_access(self, scope: &str) -> bool {
        self.allowed_scopes().contains(&scope)
    }
}
