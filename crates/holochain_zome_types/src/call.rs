use crate::capability::CapSecret;
use crate::cell::CellId;
use crate::zome::FunctionName;
use crate::zome::ZomeName;
use crate::ExternIO;
use holo_hash::AgentPubKey;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CallTargetCell {
    Other(CellId),
    Local,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CallTarget {
    Agent(AgentPubKey),
    Cell(CallTargetCell),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Call {
    pub target: CallTarget,
    pub zome_name: ZomeName,
    pub fn_name: FunctionName,
    pub cap_secret: Option<CapSecret>,
    pub payload: ExternIO,
}

impl Call {
    pub fn new(
        target: CallTarget,
        zome_name: ZomeName,
        fn_name: FunctionName,
        cap_secret: Option<CapSecret>,
        payload: ExternIO,
    ) -> Self {
        Self {
            target,
            zome_name,
            fn_name,
            cap_secret,
            payload,
        }
    }

    pub fn target(&self) -> &CallTarget {
        &self.target
    }

    pub fn zome_name(&self) -> &ZomeName {
        &self.zome_name
    }

    pub fn fn_name(&self) -> &FunctionName {
        &self.fn_name
    }

    pub fn cap_secret(&self) -> &Option<CapSecret> {
        &self.cap_secret
    }

    pub fn payload(&self) -> &ExternIO {
        &self.payload
    }
}
