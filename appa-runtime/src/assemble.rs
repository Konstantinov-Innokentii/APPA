use std::collections::BTreeMap;
use std::time::Duration;

use crate::config::{AuthorityImpl, Config};
use crate::external::AuthorityBackend;
use crate::tool::HttpClient;
use appa_engine::names::AuthorityName;

pub(crate) fn authority_backends(config: &Config) -> BTreeMap<AuthorityName, AuthorityBackend> {
    let client = HttpClient::new();
    let mut backends = BTreeMap::new();
    for authority in config.registry().authorities() {
        let backend = match config.authority_impl(&authority.name) {
            Some(AuthorityImpl::Builtin(builtin)) => AuthorityBackend::Builtin(*builtin),
            Some(AuthorityImpl::HttpResolver { url, timeout_ms }) => AuthorityBackend::Http {
                url: url.clone(),
                timeout: Duration::from_millis(*timeout_ms),
                client: client.clone(),
            },
            Some(AuthorityImpl::Hitl) | None => AuthorityBackend::Hitl,
        };
        backends.insert(authority.name.clone(), backend);
    }
    backends
}
