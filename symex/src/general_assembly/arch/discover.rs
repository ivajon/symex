use object::{Architecture, File, Object};

use super::{
    arm::{v6::ArmV6M, v7::ArmV7EM},
    Arch,
    ArchError,
    SupportedArchitechture,
};

impl SupportedArchitechture {
    /// Discovers all supported binary formats from the binary file.
    pub fn discover(obj_file: &File<'_>) -> Result<Self, ArchError> {
        let architecture = obj_file.architecture();

        match architecture {
            Architecture::Arm => {
                // Run the paths with architecture specific data.
                if let Some(v7) = ArmV7EM::discover(&obj_file)? {
                    return Ok(Self::ArmV7EM(v7));
                }

                // Run the paths with architecture specific data.
                if let Some(v6) = ArmV6M::discover(&obj_file)? {
                    return Ok(Self::ArmV6M(v6));
                }
            }
            _ => {}
        }
        Err(ArchError::UnsuportedArchitechture)
    }
}
