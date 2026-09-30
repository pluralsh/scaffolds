//! Restores an Azure Database for PostgreSQL or MySQL flexible server to a point in time.
//!
//! The restore always creates a new server next to the source, which is never changed, so
//! applications only move to the restored data once they are pointed at the new server. A
//! restore takes longer than an invocation: execute submits it, and calling the function
//! again with the same parameters reports the new server's state and hostname. The new server
//! is tagged with its source and restore point, so a server that happens to have the target
//! name is never mistaken for the restore. The restored server gets the source's network
//! settings (the same delegated subnet and private DNS zone, or public access); firewall
//! rules and private endpoints aren't copied by Azure.

use std::collections::HashMap;
use std::sync::Arc;

use functions_azure::ManagedIdentityCredential;
use functions_azure::arm::{self, Arm, Kind, Precondition, ResourceId};
use functions_core::volume::{now, parse_timestamp};
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const POSTGRES_API_VERSION: &str = "2025-08-01";
const MYSQL_API_VERSION: &str = "2024-12-30";

/// Tags on the restored server: the name of its source, which is in the same resource group,
/// and the restore point.
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
    /// Network settings, which the restored server takes over.
    #[serde(default)]
    network: Option<Value>,
}

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
    /// The restored server, once the restore was submitted.
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

async fn handle(
    client: &reqwest::Client,
    credential: &ManagedIdentityCredential,
    req: Request<Params>,
) -> Result<Response<Output>, Error> {
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

    let arm = Arm::connect(client.clone(), credential).await?;
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
            // Only creates the server: a server that appeared since the checks is left alone.
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
    // An existing restore stays valid even once its point has aged out of the backup window.
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
    json!({
        "location": source.location,
        "tags": {
            SOURCE_TAG: source_id.name,
            POINT_TAG: point,
        },
        "properties": {
            "createMode": "PointInTimeRestore",
            "sourceServerResourceId": source_id.id(),
            engine.point_property: point,
            // Whether a restore inherits the network settings isn't documented; like the
            // Azure CLI, pass the source's so it lands in the same subnet or public access.
            "network": source.properties.network,
        },
    })
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
    let credential: Arc<ManagedIdentityCredential> =
        functions_azure::credential().map_err(std::io::Error::other)?;

    functions_http::run(move |req| {
        let client = client.clone();
        let credential = credential.clone();
        async move { handle(&client, &credential, req).await }
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
