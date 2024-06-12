#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub es_password: String,
    pub es_cloud_id: String,
}

impl Config {
    pub fn new() -> Config {
        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");
        let es_password = std::env::var("ES_PASSWORD").expect("ES_PASSWORD not set");
        let es_cloud_id = std::env::var("ES_CLOUD_ID").expect("ES_CLOUD_ID not set");

        Config {
            database_url,
            es_password,
            es_cloud_id,
        }
    }
}
