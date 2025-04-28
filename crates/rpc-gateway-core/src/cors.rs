use actix_cors::Cors;

use crate::config::CorsConfig;

pub fn cors_middleware(cors_config: &CorsConfig) -> Cors {
    let mut cors = Cors::default().max_age(cors_config.max_age as usize);

    // TODO: make these configurable.
    if cors_config.allow_any_origin {
        cors = cors.allow_any_origin()
    } else {
        for origin in cors_config.allowed_origins.iter() {
            cors = cors.allowed_origin(origin);
        }
    }
    if cors_config.allow_any_header {
        cors = cors.allow_any_header()
    } else {
        for header in cors_config.allowed_headers.iter() {
            cors = cors.allowed_header(header);
        }
    }

    if cors_config.allow_any_method {
        cors = cors.allow_any_method()
    } else {
        let methods: Vec<&str> = cors_config
            .allowed_methods
            .iter()
            .map(|s| s.as_str())
            .collect();
        cors = cors.allowed_methods(methods);
    }

    if cors_config.expose_any_header {
        cors = cors.expose_any_header()
    }

    cors
}
