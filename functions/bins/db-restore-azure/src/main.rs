//! Restores an Azure Database for PostgreSQL or MySQL flexible server to a point in time.
//!
//! Restores always create a new server next to the source; the source is never changed. A
//! restore outlasts one invocation: execute submits it, and calling again with the same
//! parameters reports the new server's state and hostname.
//!
//! The new server is tagged with its source and restore point, so an unrelated server with the
//! target name is never mistaken for the restore. It copies the source's network settings,
//! availability zone, and customer managed key with its user-assigned identities. Azure doesn't
//! copy firewall rules or private endpoints.

use std::collections::HashMap;

use functions_azure::arm::{self, Connector, Kind, Precondition, ResourceId};
use functions_core::volume::{now, parse_timestamp};
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const POSTGRES_API_VERSION: &str = "2025-08-01";
const MYSQL_API_VERSION: &str = "2024-12-30";

/// Tags on the restored server: its source's name (same resource group) and the restore point.
const SOURCE_TAG: &str = "plural.sh-db-restore-source";
const POINT_TAG: &str = "plural.sh-db-restore-point";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    server_id: String,
    target_server_name: String,
    /// RFC 3339 timestamp to restore to.
    restore_point_in_time: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Server {
    #[serde(default)]
    name: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    tags: HashMap<String, String>,
    #[serde(default)]
    properties: ServerProperties,
    /// Needed by the restored server to read a customer managed key.
    #[serde(default)]
    identity: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerProperties {
    #[serde(default)]
    state: String,
    #[serde(default)]
    fully_qualified_domain_name: Option<String>,
    #[serde(default)]
    backup: Option<Backup>,
    /// Copied to the restored server.
    #[serde(default)]
    network: Option<Value>,
    /// Customer managed key settings, copied to the restored server.
    #[serde(default)]
    data_encryption: Option<Value>,
    #[serde(default)]
    availability_zone: Option<String>,
}

/// `dataEncryption` fields set at creation; the other fields only report the key's status.
const DATA_ENCRYPTION_SETTINGS: [&str; 5] = [
    "type",
    "primaryKeyURI",
    "primaryUserAssignedIdentityId",
    "geoBackupKeyURI",
    "geoBackupUserAssignedIdentityId",
];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Backup {
    #[serde(default)]
    earliest_restore_date: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerSummary {
    id: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fqdn: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<ServerSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    earliest_restore_point: Option<String>,
    /// The restored server, once submitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<ServerSummary>,
    submitted: bool,
}

/// Flexible server flavour: API version and the name of the restore point property.
struct Engine {
    api_version: &'static str,
    point_property: &'static str,
}

fn engine(kind: Kind) -> Engine {
    if kind == arm::MYSQL_FLEXIBLE_SERVER {
        Engine {
            api_version: MYSQL_API_VERSION,
            point_property: "restorePointInTime",
        }
    } else {
        Engine {
            api_version: POSTGRES_API_VERSION,
            point_property: "pointInTimeUTC",
        }
    }
}

/// What exists under the target name.
#[derive(Debug, PartialEq, Eq)]
enum Target {
    Missing,
    /// Restored from this source to this point by an earlier execute.
    Restore,
    Other,
}

async fn handle(connector: &Connector, req: Request<Params>) -> Result<Response<Output>, Error> {
    let params = req.params;
    let source_id = ResourceId::parse(
        &params.server_id,
        &[arm::POSTGRES_FLEXIBLE_SERVER, arm::MYSQL_FLEXIBLE_SERVER],
    )
    .ok_or_else(|| {
        Error::invalid_request(format!(
            "serverId {:?} is not a PostgreSQL or MySQL flexible server resource ID (/subscriptions/<id>/resourceGroups/<group>/providers/Microsoft.DBforPostgreSQL/flexibleServers/<name>)",
            params.server_id
        ))
    })?;
    let target_id = is_server_name(&params.target_server_name)
        .then(|| source_id.sibling(source_id.kind, &params.target_server_name))
        .flatten()
        .ok_or_else(|| {
            Error::invalid_request(format!(
                "targetServerName {:?} is not a valid server name (3-63 lowercase letters, digits and hyphens)",
                params.target_server_name
            ))
        })?;
    let point = parse_timestamp(&params.restore_point_in_time).ok_or_else(|| {
        Error::invalid_request(format!(
            "restorePointInTime {:?} is not an RFC 3339 timestamp",
            params.restore_point_in_time
        ))
    })?;
    let engine = engine(source_id.kind);

    let arm = connector.connect().await?;
    let source: Option<Server> = arm.get(&source_id.id(), engine.api_version).await?;
    let target_server: Option<Server> = if target_id == source_id {
        None
    } else {
        arm.get(&target_id.id(), engine.api_version).await?
    };
    let target = classify(
        target_server.as_ref(),
        &source_id,
        &params.restore_point_in_time,
    );
    let guards = evaluate(
        source.as_ref(),
        &source_id,
        &target_id,
        &target,
        point,
        now(),
    );
    let mut output = Output {
        source: source.as_ref().map(|s| summary(&source_id, s)),
        earliest_restore_point: source.as_ref().and_then(earliest_restore),
        target: target_server
            .as_ref()
            .filter(|_| target == Target::Restore)
            .map(|s| summary(&target_id, s)),
        submitted: false,
    };

    match (req.action, source) {
        (Action::Plan, _) => Ok(Response::planned(guards, output)),
        (Action::Execute, Some(source)) if guards.iter().all(|g| g.passed) => {
            if target == Target::Restore {
                return Ok(Response::done(guards, output));
            }
            let body = restore_body(&source, &source_id, &engine, &params.restore_point_in_time);
            // Create only: a server that appeared since the checks is left alone.
            arm.put(
                &target_id.id(),
                engine.api_version,
                &body,
                Precondition::IfNoneMatch,
            )
            .await?;
            tracing::info!(source = %source_id.id(), target = %target_id.id(), point = %params.restore_point_in_time, "restoring server");
            output.target = Some(ServerSummary {
                id: target_id.id(),
                state: "Provisioning".into(),
                fqdn: None,
            });
            output.submitted = true;
            Ok(Response::done(guards, output))
        }
        (Action::Execute, _) => Ok(Response::refused(guards).with_result(output)),
    }
}

/// Flexible server names: 3-63 lowercase letters, digits and hyphens, not starting or ending
/// with a hyphen.
fn is_server_name(name: &str) -> bool {
    (3..=63).contains(&name.len())
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn classify(server: Option<&Server>, source: &ResourceId, point: &str) -> Target {
    let Some(server) = server else {
        return Target::Missing;
    };
    let tag = |key: &str| server.tags.get(key).map(String::as_str);
    let from_source = tag(SOURCE_TAG).is_some_and(|s| s.eq_ignore_ascii_case(&source.name));
    if from_source && tag(POINT_TAG) == Some(point) {
        Target::Restore
    } else {
        Target::Other
    }
}

fn evaluate(
    source: Option<&Server>,
    source_id: &ResourceId,
    target_id: &ResourceId,
    target: &Target,
    point: i64,
    now: i64,
) -> Vec<Guard> {
    let mut guards = vec![match source {
        Some(s) if s.properties.state == "Ready" => {
            Guard::pass("source", format!("server {} is Ready", s.name))
        }
        Some(s) => Guard::fail(
            "source",
            format!("server {} is {}, not Ready", s.name, s.properties.state),
        ),
        None => Guard::fail("source", "server not found"),
    }];
    guards.push(if target_id == source_id {
        Guard::fail(
            "new-server",
            "the target must be a new server; restoring over the source isn't supported",
        )
    } else {
        match target {
            Target::Missing => Guard::pass(
                "new-server",
                format!("{} will be created next to the source", target_id.name),
            ),
            Target::Restore => Guard::pass(
                "new-server",
                format!(
                    "{} was already restored from this source to this point",
                    target_id.name
                ),
            ),
            Target::Other => Guard::fail(
                "new-server",
                format!(
                    "a server named {} already exists and isn't this restore; pick another name",
                    target_id.name
                ),
            ),
        }
    });
    // An existing restore stays valid even after its point leaves the backup window.
    if *target != Target::Restore {
        let earliest = source.and_then(earliest_restore);
        let after_earliest = earliest
            .as_deref()
            .and_then(parse_timestamp)
            .is_some_and(|e| point >= e);
        guards.push(Guard::check(
            "restore-point",
            after_earliest && point <= now,
            format!(
                "must be between the earliest restore point ({}) and now",
                earliest.as_deref().unwrap_or("unknown")
            ),
        ));
    }
    guards
}

/// PUT body creating the restored server next to the source.
fn restore_body(source: &Server, source_id: &ResourceId, engine: &Engine, point: &str) -> Value {
    let mut body = json!({
        "location": source.location,
        "tags": {
            SOURCE_TAG: source_id.name,
            POINT_TAG: point,
        },
        "properties": {
            "createMode": "PointInTimeRestore",
            "sourceServerResourceId": source_id.id(),
            engine.point_property: point,
            // Azure doesn't document whether a restore inherits the network settings. Like
            // the Azure CLI, pass the source's.
            "network": source.properties.network,
        },
    });
    // Like the Azure CLI, keep the source's zone, key and key-reading identities. A server
    // encrypted with a customer managed key can't be restored without them.
    if let Some(zone) = source
        .properties
        .availability_zone
        .as_deref()
        .filter(|z| !z.is_empty())
    {
        body["properties"]["availabilityZone"] = json!(zone);
    }
    if let Some(encryption) = source
        .properties
        .data_encryption
        .as_ref()
        .and_then(Value::as_object)
    {
        let settings: serde_json::Map<String, Value> = DATA_ENCRYPTION_SETTINGS
            .iter()
            .filter_map(|key| {
                let value = encryption.get(*key).filter(|v| !v.is_null())?;
                Some(((*key).to_owned(), value.clone()))
            })
            .collect();
        if !settings.is_empty() {
            body["properties"]["dataEncryption"] = Value::Object(settings);
        }
    }
    if let Some(identity) = source.identity.as_ref().and_then(restore_identity) {
        body["identity"] = identity;
    }
    body
}

/// Identity for the restored server: the source's user-assigned identities (by ID only), plus
/// its own system-assigned identity if the source has one.
fn restore_identity(identity: &Value) -> Option<Value> {
    let kind = identity.get("type")?.as_str()?;
    if kind.eq_ignore_ascii_case("None") {
        return None;
    }
    let mut out = json!({ "type": kind });
    if let Some(assigned) = identity
        .get("userAssignedIdentities")
        .and_then(Value::as_object)
    {
        out["userAssignedIdentities"] = assigned.keys().map(|id| (id.clone(), json!({}))).collect();
    }
    Some(out)
}

fn earliest_restore(server: &Server) -> Option<String> {
    server
        .properties
        .backup
        .as_ref()
        .and_then(|b| b.earliest_restore_date.clone())
}

fn summary(id: &ResourceId, server: &Server) -> ServerSummary {
    ServerSummary {
        id: id.id(),
        state: server.properties.state.clone(),
        fqdn: server.properties.fully_qualified_domain_name.clone(),
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let client = functions_http::https_client().map_err(std::io::Error::other)?;
    let connector = Connector::from_env(client).map_err(std::io::Error::other)?;

    functions_http::run(move |req| {
        let connector = connector.clone();
        async move { handle(&connector, req).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUB: &str = "00000000-0000-0000-0000-000000000000";
    const POINT: &str = "2026-09-30T08:00:00Z";

    fn id(name: &str) -> ResourceId {
        ResourceId::parse(
            &format!("/subscriptions/{SUB}/resourceGroups/db/providers/Microsoft.DBforPostgreSQL/flexibleServers/{name}"),
            &[arm::POSTGRES_FLEXIBLE_SERVER],
        )
        .unwrap()
    }

    fn server(name: &str, state: &str, tags: &[(&str, &str)]) -> Server {
        serde_json::from_value(json!({
            "name": name, "location": "eastus",
            "tags": tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>(),
            "properties": {"state": state, "fullyQualifiedDomainName": format!("{name}.postgres.database.azure.com"),
                           "network": {"delegatedSubnetResourceId": "/x/subnets/db", "publicNetworkAccess": "Disabled"},
                           "backup": {"earliestRestoreDate": "2026-09-23T08:00:00+00:00"}}
        }))
        .unwrap()
    }

    fn at(ts: &str) -> i64 {
        parse_timestamp(ts).unwrap()
    }

    fn failed(guards: &[Guard]) -> Vec<&str> {
        guards
            .iter()
            .filter(|g| !g.passed)
            .map(|g| g.name.as_str())
            .collect()
    }

    fn check(target: Option<&Server>, target_name: &str, point: &str) -> Vec<Guard> {
        let source = server("app", "Ready", &[]);
        let kind = classify(target, &id("app"), point);
        evaluate(
            Some(&source),
            &id("app"),
            &id(target_name),
            &kind,
            at(point),
            at("2026-09-30T12:00:00Z"),
        )
    }

    #[test]
    fn restores_within_the_backup_window_to_a_new_server() {
        assert!(failed(&check(None, "app-restored", POINT)).is_empty());
        assert_eq!(
            failed(&check(None, "app-restored", "2026-09-20T08:00:00Z")),
            ["restore-point"]
        );
        assert_eq!(
            failed(&check(None, "app-restored", "2026-10-01T08:00:00Z")),
            ["restore-point"]
        );
    }

    #[test]
    fn never_restores_over_the_source_or_another_server() {
        assert_eq!(failed(&check(None, "app", POINT)), ["new-server"]);
        let other = server("app-restored", "Ready", &[]);
        assert_eq!(
            failed(&check(Some(&other), "app-restored", POINT)),
            ["new-server"]
        );
    }

    #[test]
    fn recognizes_its_own_restore() {
        let source = "app".to_owned();
        let restored = server(
            "app-restored",
            "Provisioning",
            &[(SOURCE_TAG, &source), (POINT_TAG, POINT)],
        );
        let other_point = server(
            "app-restored",
            "Ready",
            &[(SOURCE_TAG, &source), (POINT_TAG, "2026-09-29T08:00:00Z")],
        );

        assert_eq!(
            classify(Some(&restored), &id("app"), POINT),
            Target::Restore
        );
        assert_eq!(
            classify(Some(&other_point), &id("app"), POINT),
            Target::Other
        );
        let guards = check(Some(&restored), "app-restored", POINT);
        assert!(failed(&guards).is_empty());
        assert!(guards.iter().all(|g| g.name != "restore-point"));
    }

    #[test]
    fn requires_a_ready_source() {
        let stopped = server("app", "Stopped", &[]);
        let guards = evaluate(
            Some(&stopped),
            &id("app"),
            &id("app-restored"),
            &Target::Missing,
            at(POINT),
            at("2026-09-30T12:00:00Z"),
        );

        assert_eq!(failed(&guards), ["source"]);
    }

    #[test]
    fn validates_server_names() {
        for ok in ["app", "app-restored-2", &"a".repeat(63)] {
            assert!(is_server_name(ok), "{ok}");
        }
        for bad in [
            "ab",
            "-app",
            "app-",
            "App",
            "app_1",
            "app.1",
            &"a".repeat(64),
        ] {
            assert!(!is_server_name(bad), "{bad}");
        }
    }

    #[test]
    fn restore_body_tags_the_new_server() {
        let source = server("app", "Ready", &[]);
        let body = restore_body(
            &source,
            &id("app"),
            &engine(arm::POSTGRES_FLEXIBLE_SERVER),
            POINT,
        );

        assert_eq!(
            body,
            json!({
                "location": "eastus",
                "tags": {SOURCE_TAG: "app", POINT_TAG: POINT},
                "properties": {
                    "createMode": "PointInTimeRestore",
                    "sourceServerResourceId": id("app").id(),
                    "pointInTimeUTC": POINT,
                    "network": {"delegatedSubnetResourceId": "/x/subnets/db", "publicNetworkAccess": "Disabled"},
                },
            })
        );
        assert!(body["tags"].get("plural.sh-db-restore-source").is_some());
    }

    #[test]
    fn restore_body_keeps_the_zone_key_and_identities() {
        let uami = "/subscriptions/s/resourceGroups/rg/providers/Microsoft.ManagedIdentity/userAssignedIdentities/db";
        let mut source = server("app", "Ready", &[]);
        source.properties.availability_zone = Some("2".into());
        source.properties.data_encryption = Some(json!({
            "type": "AzureKeyVault",
            "primaryKeyURI": "https://kv.vault.azure.net/keys/db/1",
            "primaryUserAssignedIdentityId": uami,
            "geoBackupKeyURI": null,
            "primaryEncryptionKeyStatus": "Valid",
        }));
        source.identity = Some(json!({
            "type": "UserAssigned",
            "tenantId": "t",
            "userAssignedIdentities": {uami: {"principalId": "p", "clientId": "c"}},
        }));

        for kind in [arm::POSTGRES_FLEXIBLE_SERVER, arm::MYSQL_FLEXIBLE_SERVER] {
            let body = restore_body(&source, &id("app"), &engine(kind), POINT);

            assert_eq!(body["properties"]["availabilityZone"], "2");
            assert_eq!(
                body["properties"]["dataEncryption"],
                json!({
                    "type": "AzureKeyVault",
                    "primaryKeyURI": "https://kv.vault.azure.net/keys/db/1",
                    "primaryUserAssignedIdentityId": uami,
                })
            );
            assert_eq!(
                body["identity"],
                json!({"type": "UserAssigned", "userAssignedIdentities": {uami: {}}})
            );
        }
    }

    #[test]
    fn restore_body_leaves_out_what_the_source_doesnt_have() {
        let mut source = server("app", "Ready", &[]);
        source.properties.availability_zone = Some(String::new());
        source.identity = Some(json!({"type": "None"}));

        let body = restore_body(
            &source,
            &id("app"),
            &engine(arm::POSTGRES_FLEXIBLE_SERVER),
            POINT,
        );

        assert!(body.get("identity").is_none(), "{body}");
        assert!(
            body["properties"].get("availabilityZone").is_none(),
            "{body}"
        );
        assert!(body["properties"].get("dataEncryption").is_none(), "{body}");
    }

    #[test]
    fn uses_each_engines_restore_property() {
        assert_eq!(
            engine(arm::MYSQL_FLEXIBLE_SERVER).point_property,
            "restorePointInTime"
        );
        assert_eq!(
            engine(arm::POSTGRES_FLEXIBLE_SERVER).point_property,
            "pointInTimeUTC"
        );
    }
}

#[cfg(test)]
mod handler_tests {
    use functions_azure::mock::{Method, MockArm};

    use super::*;

    const RG: &str =
        "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/db/providers";
    const POINT: &str = "2021-01-01T00:00:00Z";

    fn path(engine: &str, name: &str) -> String {
        format!("{RG}/Microsoft.DBfor{engine}/flexibleServers/{name}")
    }

    fn server(state: &str, tags: Value) -> Value {
        json!({
            "name": "app", "location": "eastus", "tags": tags,
            "properties": {
                "state": state, "fullyQualifiedDomainName": "app.example",
                "backup": {"earliestRestoreDate": "2020-01-01T00:00:00+00:00"},
                "network": {"delegatedSubnetResourceId": "/x/subnets/db", "publicNetworkAccess": "Disabled"},
            }
        })
    }

    async fn call(mock: &MockArm, engine: &str, action: &str, point: &str) -> Value {
        let req = serde_json::from_value(json!({
            "action": action, "serverId": path(engine, "app"),
            "targetServerName": "app-restored", "restorePointInTime": point,
        }))
        .unwrap();
        serde_json::to_value(handle(&mock.connector(), req).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn plan_reads_only() {
        let mock = MockArm::start().await;
        mock.on(
            Method::GET,
            &path("PostgreSQL", "app"),
            200,
            server("Ready", json!({})),
        );

        let resp = call(&mock, "PostgreSQL", "plan", POINT).await;

        assert_eq!(resp["outcome"], "planned", "{resp}");
        assert_eq!(
            resp["result"]["earliestRestorePoint"],
            "2020-01-01T00:00:00+00:00"
        );
        assert!(mock.writes().is_empty());
    }

    #[tokio::test]
    async fn execute_creates_the_new_server_only() {
        for (engine, version, property) in [
            ("PostgreSQL", POSTGRES_API_VERSION, "pointInTimeUTC"),
            ("MySQL", MYSQL_API_VERSION, "restorePointInTime"),
        ] {
            let mock = MockArm::start().await;
            mock.on(
                Method::GET,
                &path(engine, "app"),
                200,
                server("Ready", json!({})),
            )
            .on(Method::PUT, &path(engine, "app-restored"), 201, json!({}));

            let resp = call(&mock, engine, "execute", POINT).await;

            assert_eq!(resp["outcome"], "done", "{engine}: {resp}");
            let writes = mock.writes();
            assert_eq!(writes.len(), 1, "{engine}");
            let put = &writes[0];
            assert!(
                put.path.ends_with("/flexibleServers/app-restored"),
                "{engine}"
            );
            assert!(
                put.query.contains(&format!("api-version={version}")),
                "{engine}"
            );
            assert_eq!(put.if_none_match.as_deref(), Some("*"), "{engine}");
            assert_eq!(put.body["properties"][property], POINT, "{engine}");
            assert_eq!(put.body["properties"]["createMode"], "PointInTimeRestore");
            assert_eq!(
                put.body["properties"]["network"]["delegatedSubnetResourceId"],
                "/x/subnets/db"
            );
            assert_eq!(put.body["tags"][SOURCE_TAG], "app");
        }
    }

    #[tokio::test]
    async fn reports_its_restore_without_submitting_again() {
        let mock = MockArm::start().await;
        mock.on(
            Method::GET,
            &path("PostgreSQL", "app"),
            200,
            server("Ready", json!({})),
        )
        .on(
            Method::GET,
            &path("PostgreSQL", "app-restored"),
            200,
            server("Provisioning", json!({SOURCE_TAG: "app", POINT_TAG: POINT})),
        );

        let resp = call(&mock, "PostgreSQL", "execute", POINT).await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(resp["result"]["submitted"], false);
        assert_eq!(resp["result"]["target"]["state"], "Provisioning");
        assert!(mock.writes().is_empty());
    }

    #[tokio::test]
    async fn refuses_existing_servers_and_points_outside_the_window() {
        let taken = MockArm::start().await;
        taken
            .on(
                Method::GET,
                &path("PostgreSQL", "app"),
                200,
                server("Ready", json!({})),
            )
            .on(
                Method::GET,
                &path("PostgreSQL", "app-restored"),
                200,
                server("Ready", json!({})),
            );
        assert_eq!(
            call(&taken, "PostgreSQL", "execute", POINT).await["outcome"],
            "refused"
        );
        assert!(taken.writes().is_empty());

        let early = MockArm::start().await;
        early.on(
            Method::GET,
            &path("PostgreSQL", "app"),
            200,
            server("Ready", json!({})),
        );
        assert_eq!(
            call(&early, "PostgreSQL", "execute", "2019-01-01T00:00:00Z").await["outcome"],
            "refused"
        );
        assert!(early.writes().is_empty());
    }
}
