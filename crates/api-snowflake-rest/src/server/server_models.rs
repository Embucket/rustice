use crate::models::Auth;
use executor::utils::DataSerializationFormat;

#[derive(Clone, Default)]
pub struct RestApiConfig {
    pub auth: Auth,
    pub dbt_serialization_format: DataSerializationFormat,
}

impl RestApiConfig {
    pub fn new(data_format: &str, jwt_secret: String) -> Result<Self, strum::ParseError> {
        Ok(Self {
            dbt_serialization_format: DataSerializationFormat::try_from(data_format)?,
            auth: Auth::new(jwt_secret),
        })
    }
    #[must_use]
    pub fn with_demo_credentials(mut self, demo_user: String, demo_password: String) -> Self {
        self.auth = Auth {
            demo_user,
            demo_password,
            ..self.auth
        };
        self
    }

    #[must_use]
    pub const fn with_trust_spcs_ingress(mut self, trust_spcs_ingress: bool) -> Self {
        self.auth.trust_spcs_ingress = trust_spcs_ingress;
        self
    }
}
