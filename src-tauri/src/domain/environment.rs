use serde::{Deserialize, Serialize};
use ts_rs::TS;

macro_rules! binding_path {
    () => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings/")
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct RuntimeCheck {
    pub available: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct RuntimeStatus {
    pub python: RuntimeCheck,
    pub node: RuntimeCheck,
    pub git: RuntimeCheck,
}
