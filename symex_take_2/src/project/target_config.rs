//! Information about the specific target hardware supplied by the user.

pub struct TargetConfiguration {
    core: CoreFamily,
}

pub enum CoreFamily {
    ArmM(ArmMCore),
}

pub enum ArmMCore {
    ArmM0,
    ArmM0Plus,
}