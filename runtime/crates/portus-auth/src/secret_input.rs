use portus_protected_api::SecretMaterial;
use std::io::IsTerminal;

pub trait SecretReader {
    fn read_secret(&mut self, prompt: &str) -> Result<SecretMaterial, String>;
}

#[derive(Default)]
pub struct SystemSecretReader;

impl SecretReader for SystemSecretReader {
    fn read_secret(&mut self, prompt: &str) -> Result<SecretMaterial, String> {
        if !std::io::stdin().is_terminal() {
            return Err(
                "protected credential entry requires an interactive TTY; secret pipelines/stdin are intentionally rejected"
                    .into(),
            );
        }
        let value = rpassword::prompt_password(prompt)
            .map_err(|_| "protected credential could not be read from TTY".to_string())?;
        SecretMaterial::new(value).map_err(str::to_string)
    }
}
