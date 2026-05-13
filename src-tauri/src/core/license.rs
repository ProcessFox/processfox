use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Edition {
    Community,
    Store,
}

impl Edition {
    pub fn current() -> Self {
        if cfg!(edition_store) {
            Edition::Store
        } else {
            Edition::Community
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Edition::Community => "Community",
            Edition::Store => "Store",
        }
    }

    pub fn license_spdx(&self) -> &'static str {
        match self {
            Edition::Community => "GPL-3.0-or-later",
            Edition::Store => "LicenseRef-ProcessFox-Store",
        }
    }
}
