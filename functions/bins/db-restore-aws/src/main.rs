//! Restores an Amazon RDS DB instance to a point in time.
//!
//! Restores always create a new instance next to the source; the source is never changed. A
//! restore outlasts one invocation: execute submits it, and calling again with the same
//! parameters reports the new instance's state and endpoint. Aurora DB instances aren't
//! supported (`RestoreDBInstanceToPointInTime` doesn't apply to them).
//!
//! The new instance is tagged with its source and restore point, so an unrelated instance with
//! the target identifier is never mistaken for the restore. RDS copies the source's
//! configuration (subnet group, security groups, parameter group, storage, and encryption) onto
//! the new instance; option groups and some networking details may still need a follow-up.

use std::collections::HashMap;

use aws_sdk_rds::Client;
use aws_sdk_rds::error::ProvideErrorMetadata;
use aws_sdk_rds::types::{DbInstance as SdkDbInstance, Tag};
use aws_smithy_types::DateTime;
use functions_aws::provider_error;
use functions_core::volume::{now, parse_timestamp};
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};

/// Tags on the restored instance: the source's identifier and the restore point.
const SOURCE_TAG: &str = "plural.sh-db-restore-source";
const POINT_TAG: &str = "plural.sh-db-restore-point";

type Tags = HashMap<String, String>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    db_instance_identifier: String,
    target_db_instance_identifier: String,
    /// RFC 3339 timestamp to restore to.
    restore_point_in_time: String,
}

#[derive(Debug, Clone)]
struct DbInstance {
    identifier: String,
    arn: String,
    status: String,
    /// Set for Aurora instances, which this function refuses.
    cluster: Option<String>,
    earliest_restore: Option<i64>,
    latest_restore: Option<i64>,
    endpoint: Option<String>,
    tags: Tags,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceSummary {
    identifier: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<InstanceSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    earliest_restore_point: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_restore_point: Option<String>,
    /// The restored instance, once submitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<InstanceSummary>,
    submitted: bool,
}

/// What exists under the target identifier.
#[derive(Debug, PartialEq, Eq)]
enum Target {
    Missing,
    /// Restored from this source to this point by an earlier execute.
    Restore,
    Other,
}

fn validate(params: &Params) -> Result<i64, Error> {
    if !is_db_identifier(&params.db_instance_identifier) {
        return Err(Error::invalid_request(format!(
            "dbInstanceIdentifier {:?} is not a valid RDS DB instance identifier",
            params.db_instance_identifier
        )));
    }
    if !is_db_identifier(&params.target_db_instance_identifier) {
        return Err(Error::invalid_request(format!(
            "targetDbInstanceIdentifier {:?} is not a valid RDS DB instance identifier",
            params.target_db_instance_identifier
        )));
    }
    parse_timestamp(&params.restore_point_in_time).ok_or_else(|| {
        Error::invalid_request(format!(
            "restorePointInTime {:?} is not an RFC 3339 timestamp",
            params.restore_point_in_time
        ))
    })
}

/// RDS DB instance identifiers: 1-63 letters, digits and hyphens; start with a letter; no
/// trailing hyphen; no consecutive hyphens.
fn is_db_identifier(name: &str) -> bool {
    (1..=63).contains(&name.len())
        && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && !name.ends_with('-')
        && !name.contains("--")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

async fn handle(rds: &Client, req: Request<Params>) -> Result<Response<Output>, Error> {
    let params = req.params;
    let point = validate(&params)?;

    let source = find_instance(rds, &params.db_instance_identifier).await?;
    let target_instance = if params.target_db_instance_identifier == params.db_instance_identifier {
        None
    } else {
        find_instance(rds, &params.target_db_instance_identifier).await?
    };
    let target = classify(
        target_instance.as_ref(),
        &params.db_instance_identifier,
        &params.restore_point_in_time,
    );
    let guards = evaluate(
        source.as_ref(),
        &params.db_instance_identifier,
        &params.target_db_instance_identifier,
        &target,
        point,
        now(),
    );
    let mut output = Output {
        source: source.as_ref().map(summary),
        earliest_restore_point: source
            .as_ref()
            .and_then(|s| s.earliest_restore)
            .and_then(format_timestamp),
        latest_restore_point: source
            .as_ref()
            .and_then(|s| s.latest_restore)
            .and_then(format_timestamp),
        target: target_instance
            .as_ref()
            .filter(|_| target == Target::Restore)
            .map(summary),
        submitted: false,
    };

    match (req.action, source) {
        (Action::Plan, _) => Ok(Response::planned(guards, output)),
        (Action::Execute, Some(_)) if guards.iter().all(|g| g.passed) => {
            if target == Target::Restore {
                return Ok(Response::done(guards, output));
            }
            rds.restore_db_instance_to_point_in_time()
                .source_db_instance_identifier(&params.db_instance_identifier)
                .target_db_instance_identifier(&params.target_db_instance_identifier)
                .restore_time(DateTime::from_secs(point))
                .tags(
                    Tag::builder()
                        .key(SOURCE_TAG)
                        .value(&params.db_instance_identifier)
                        .build(),
                )
                .tags(
                    Tag::builder()
                        .key(POINT_TAG)
                        .value(&params.restore_point_in_time)
                        .build(),
                )
                .send()
                .await
                .map_err(provider_error)?;
            tracing::info!(
                source = %params.db_instance_identifier,
                target = %params.target_db_instance_identifier,
                point = %params.restore_point_in_time,
                "restoring DB instance"
            );
            output.target = Some(InstanceSummary {
                identifier: params.target_db_instance_identifier.clone(),
                status: "creating".into(),
                endpoint: None,
            });
            output.submitted = true;
            Ok(Response::done(guards, output))
        }
        (Action::Execute, _) => Ok(Response::refused(guards).with_result(output)),
    }
}

async fn find_instance(rds: &Client, id: &str) -> Result<Option<DbInstance>, Error> {
    let out = match rds
        .describe_db_instances()
        .db_instance_identifier(id)
        .send()
        .await
    {
        Ok(out) => out,
        Err(err) if err.code() == Some("DBInstanceNotFound") => return Ok(None),
        Err(err) => return Err(provider_error(err)),
    };
    let Some(instance) = out.db_instances().first() else {
        return Ok(None);
    };
    let mut db = db_from(instance);
    if !db.arn.is_empty() {
        db.tags = tags_of(rds, &db.arn).await?;
    }
    Ok(Some(db))
}

fn db_from(instance: &SdkDbInstance) -> DbInstance {
    let latest_restore = instance.latest_restorable_time().map(|t| t.secs());
    let created = instance.instance_create_time().map(|t| t.secs());
    // DescribeDBInstances reports LatestRestorableTime but not EarliestRestorableTime (that
    // field is on clusters). Derive the start of the window from backup retention.
    let earliest_restore = match (latest_restore, instance.backup_retention_period()) {
        (Some(latest), Some(days)) if days > 0 => {
            let from_retention = latest.saturating_sub(i64::from(days) * 86_400);
            Some(created.map_or(from_retention, |c| c.max(from_retention)))
        }
        _ => None,
    };
    DbInstance {
        identifier: instance
            .db_instance_identifier()
            .unwrap_or_default()
            .to_owned(),
        arn: instance.db_instance_arn().unwrap_or_default().to_owned(),
        status: instance.db_instance_status().unwrap_or_default().to_owned(),
        cluster: instance.db_cluster_identifier().map(str::to_owned),
        earliest_restore,
        latest_restore,
        endpoint: instance
            .endpoint()
            .and_then(|e| e.address().map(str::to_owned)),
        tags: Tags::new(),
    }
}

async fn tags_of(rds: &Client, arn: &str) -> Result<Tags, Error> {
    let out = rds
        .list_tags_for_resource()
        .resource_name(arn)
        .send()
        .await
        .map_err(provider_error)?;
    Ok(out
        .tag_list()
        .iter()
        .filter_map(|tag| {
            Some((
                tag.key()?.to_owned(),
                tag.value().unwrap_or_default().to_owned(),
            ))
        })
        .collect())
}

fn classify(instance: Option<&DbInstance>, source: &str, point: &str) -> Target {
    let Some(instance) = instance else {
        return Target::Missing;
    };
    let tag = |key: &str| instance.tags.get(key).map(String::as_str);
    let from_source = tag(SOURCE_TAG).is_some_and(|s| s.eq_ignore_ascii_case(source));
    if from_source && tag(POINT_TAG) == Some(point) {
        Target::Restore
    } else {
        Target::Other
    }
}

fn evaluate(
    source: Option<&DbInstance>,
    source_id: &str,
    target_id: &str,
    target: &Target,
    point: i64,
    now: i64,
) -> Vec<Guard> {
    let mut guards = vec![match source {
        Some(s) if s.cluster.is_some() => Guard::fail(
            "source",
            format!(
                "DB instance {} belongs to Aurora cluster {}; RestoreDBInstanceToPointInTime doesn't apply to Aurora",
                s.identifier,
                s.cluster.as_deref().unwrap_or_default()
            ),
        ),
        Some(s) if s.status == "available" => Guard::pass(
            "source",
            format!("DB instance {} is available", s.identifier),
        ),
        Some(s) => Guard::fail(
            "source",
            format!(
                "DB instance {} is {}, not available",
                s.identifier, s.status
            ),
        ),
        None => Guard::fail("source", "DB instance not found"),
    }];
    guards.push(if target_id == source_id {
        Guard::fail(
            "new-server",
            "the target must be a new DB instance; restoring over the source isn't supported",
        )
    } else {
        match target {
            Target::Missing => Guard::pass(
                "new-server",
                format!("{target_id} will be created next to the source"),
            ),
            Target::Restore => Guard::pass(
                "new-server",
                format!("{target_id} was already restored from this source to this point"),
            ),
            Target::Other => Guard::fail(
                "new-server",
                format!(
                    "a DB instance named {target_id} already exists and isn't this restore; pick another name"
                ),
            ),
        }
    });
    // An existing restore stays valid even after its point leaves the backup window.
    if *target != Target::Restore {
        let earliest = source.and_then(|s| s.earliest_restore);
        let latest = source.and_then(|s| s.latest_restore).unwrap_or(now);
        let after_earliest = earliest.is_some_and(|e| point >= e);
        let before_latest = point <= latest;
        guards.push(Guard::check(
            "restore-point",
            after_earliest && before_latest,
            format!(
                "must be between the earliest restore point ({}) and the latest ({})",
                earliest
                    .and_then(format_timestamp)
                    .as_deref()
                    .unwrap_or("unknown"),
                format_timestamp(latest).unwrap_or_else(|| "unknown".into()),
            ),
        ));
    }
    guards
}

fn summary(instance: &DbInstance) -> InstanceSummary {
    InstanceSummary {
        identifier: instance.identifier.clone(),
        status: instance.status.clone(),
        endpoint: instance.endpoint.clone(),
    }
}

fn format_timestamp(secs: i64) -> Option<String> {
    time::OffsetDateTime::from_unix_timestamp(secs)
        .ok()?
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}

#[tokio::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    let rds = Client::new(&functions_aws::sdk_config().await);

    functions_aws::run(move |req| {
        let rds = rds.clone();
        async move { handle(&rds, req).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const POINT: &str = "2026-09-30T08:00:00Z";

    fn instance(
        id: &str,
        status: &str,
        tags: &[(&str, &str)],
        earliest: Option<i64>,
        latest: Option<i64>,
    ) -> DbInstance {
        DbInstance {
            identifier: id.into(),
            arn: format!("arn:aws:rds:us-east-2:123456789012:db:{id}"),
            status: status.into(),
            cluster: None,
            earliest_restore: earliest,
            latest_restore: latest,
            endpoint: Some(format!("{id}.xyz.us-east-2.rds.amazonaws.com")),
            tags: tags
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
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

    fn check(target: Option<&DbInstance>, target_name: &str, point: &str) -> Vec<Guard> {
        let source = instance(
            "app",
            "available",
            &[],
            Some(at("2026-09-23T08:00:00Z")),
            Some(at("2026-09-30T12:00:00Z")),
        );
        let kind = classify(target, "app", point);
        evaluate(
            Some(&source),
            "app",
            target_name,
            &kind,
            at(point),
            at("2026-09-30T12:00:00Z"),
        )
    }

    #[test]
    fn restores_within_the_backup_window_to_a_new_instance() {
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
    fn never_restores_over_the_source_or_another_instance() {
        assert_eq!(failed(&check(None, "app", POINT)), ["new-server"]);
        let other = instance("app-restored", "available", &[], None, None);
        assert_eq!(
            failed(&check(Some(&other), "app-restored", POINT)),
            ["new-server"]
        );
    }

    #[test]
    fn recognizes_its_own_restore() {
        let restored = instance(
            "app-restored",
            "creating",
            &[(SOURCE_TAG, "app"), (POINT_TAG, POINT)],
            None,
            None,
        );
        let other_point = instance(
            "app-restored",
            "available",
            &[(SOURCE_TAG, "app"), (POINT_TAG, "2026-09-29T08:00:00Z")],
            None,
            None,
        );

        assert_eq!(classify(Some(&restored), "app", POINT), Target::Restore);
        assert_eq!(classify(Some(&other_point), "app", POINT), Target::Other);
        let guards = check(Some(&restored), "app-restored", POINT);
        assert!(failed(&guards).is_empty());
        assert!(guards.iter().all(|g| g.name != "restore-point"));
    }

    #[test]
    fn requires_an_available_non_aurora_source() {
        let stopped = instance("app", "stopped", &[], Some(at(POINT)), Some(at(POINT)));
        let guards = evaluate(
            Some(&stopped),
            "app",
            "app-restored",
            &Target::Missing,
            at(POINT),
            at("2026-09-30T12:00:00Z"),
        );
        assert_eq!(failed(&guards), ["source"]);

        let mut aurora = instance("app", "available", &[], Some(at(POINT)), Some(at(POINT)));
        aurora.cluster = Some("app-cluster".into());
        let guards = evaluate(
            Some(&aurora),
            "app",
            "app-restored",
            &Target::Missing,
            at(POINT),
            at("2026-09-30T12:00:00Z"),
        );
        assert_eq!(failed(&guards), ["source"]);
    }

    #[test]
    fn validates_db_identifiers() {
        for ok in ["app", "App-1", "a", &"a".repeat(63)] {
            assert!(is_db_identifier(ok), "{ok}");
        }
        for bad in [
            "",
            "-app",
            "app-",
            "app--1",
            "app_1",
            "app.1",
            &"a".repeat(64),
        ] {
            assert!(!is_db_identifier(bad), "{bad}");
        }
    }
}

#[cfg(test)]
mod handler_tests {
    use aws_sdk_rds::error::ErrorMetadata;
    use aws_sdk_rds::operation::describe_db_instances::{
        DescribeDBInstancesError, DescribeDbInstancesOutput,
    };
    use aws_sdk_rds::operation::list_tags_for_resource::ListTagsForResourceOutput;
    use aws_sdk_rds::operation::restore_db_instance_to_point_in_time::RestoreDbInstanceToPointInTimeOutput;
    use aws_sdk_rds::types::{DbInstance, Endpoint, Tag};
    use aws_smithy_mocks::{MockResponse, Rule, RuleMode, mock, mock_client};
    use aws_smithy_types::DateTime;
    use serde_json::{Value, json};

    use super::*;

    const SOURCE: &str = "app";
    const TARGET: &str = "app-restored";
    const POINT: &str = "2026-09-30T08:00:00Z";
    const SOURCE_ARN: &str = "arn:aws:rds:us-east-2:123456789012:db:app";
    const TARGET_ARN: &str = "arn:aws:rds:us-east-2:123456789012:db:app-restored";

    struct Rds {
        client: Client,
        restore: Rule,
    }

    impl Rds {
        fn new(
            source: Option<DbInstance>,
            target: Option<(DbInstance, Vec<(&'static str, &'static str)>)>,
        ) -> Self {
            let source_id = source
                .as_ref()
                .and_then(|i| i.db_instance_identifier().map(str::to_owned));
            let target_id = target
                .as_ref()
                .and_then(|(i, _)| i.db_instance_identifier().map(str::to_owned));
            let target_tags = target
                .as_ref()
                .map(|(_, tags)| tags.clone())
                .unwrap_or_default();
            let target_instance = target.map(|(i, _)| i);

            let describe = mock!(Client::describe_db_instances).then_compute_response(move |req| {
                let id = req.db_instance_identifier().unwrap_or_default();
                if Some(id) == source_id.as_deref() {
                    return MockResponse::Output(
                        DescribeDbInstancesOutput::builder()
                            .db_instances(source.clone().unwrap())
                            .build(),
                    );
                }
                if Some(id) == target_id.as_deref() {
                    return MockResponse::Output(
                        DescribeDbInstancesOutput::builder()
                            .db_instances(target_instance.clone().unwrap())
                            .build(),
                    );
                }
                MockResponse::Error(DescribeDBInstancesError::generic(
                    ErrorMetadata::builder().code("DBInstanceNotFound").build(),
                ))
            });

            let list_tags = mock!(Client::list_tags_for_resource).then_compute_output(move |req| {
                let name = req.resource_name().unwrap_or_default();
                let tags = if name == TARGET_ARN {
                    target_tags
                        .iter()
                        .map(|(k, v)| Tag::builder().key(*k).value(*v).build())
                        .collect()
                } else {
                    vec![]
                };
                ListTagsForResourceOutput::builder()
                    .set_tag_list(Some(tags))
                    .build()
            });

            let restore = mock!(Client::restore_db_instance_to_point_in_time)
                .match_requests(move |req| {
                    req.source_db_instance_identifier() == Some(SOURCE)
                        && req.target_db_instance_identifier() == Some(TARGET)
                })
                .then_output(|| {
                    RestoreDbInstanceToPointInTimeOutput::builder()
                        .db_instance(
                            DbInstance::builder()
                                .db_instance_identifier(TARGET)
                                .db_instance_status("creating")
                                .build(),
                        )
                        .build()
                });

            let client = mock_client!(
                aws_sdk_rds,
                RuleMode::MatchAny,
                &[describe, list_tags, restore.clone()]
            );
            Self { client, restore }
        }

        async fn call(&self, action: &str) -> Value {
            let req = serde_json::from_value(json!({
                "action": action,
                "dbInstanceIdentifier": SOURCE,
                "targetDbInstanceIdentifier": TARGET,
                "restorePointInTime": POINT,
            }))
            .unwrap();
            serde_json::to_value(handle(&self.client, req).await.unwrap()).unwrap()
        }
    }

    fn available_source() -> DbInstance {
        DbInstance::builder()
            .db_instance_identifier(SOURCE)
            .db_instance_arn(SOURCE_ARN)
            .db_instance_status("available")
            .engine("postgres")
            .instance_create_time(DateTime::from_secs(
                parse_timestamp("2026-09-23T08:00:00Z").unwrap(),
            ))
            .backup_retention_period(7)
            .latest_restorable_time(DateTime::from_secs(
                parse_timestamp("2026-09-30T12:00:00Z").unwrap(),
            ))
            .endpoint(
                Endpoint::builder()
                    .address("app.xyz.us-east-2.rds.amazonaws.com")
                    .build(),
            )
            .build()
    }

    fn failed(resp: &Value) -> Vec<&str> {
        resp["guards"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|g| g["passed"] == false)
            .map(|g| g["name"].as_str().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn plan_reports_the_window_without_restoring() {
        let rds = Rds::new(Some(available_source()), None);
        let resp = rds.call("plan").await;
        assert_eq!(resp["outcome"], "planned");
        assert!(failed(&resp).is_empty());
        assert_eq!(resp["result"]["submitted"], false);
        assert_eq!(rds.restore.num_calls(), 0);
    }

    #[tokio::test]
    async fn execute_submits_the_restore_once() {
        let rds = Rds::new(Some(available_source()), None);
        let resp = rds.call("execute").await;
        assert_eq!(resp["outcome"], "done");
        assert_eq!(resp["result"]["submitted"], true);
        assert_eq!(resp["result"]["target"]["status"], "creating");
        assert_eq!(rds.restore.num_calls(), 1);
    }

    #[tokio::test]
    async fn execute_is_idempotent_for_its_own_restore() {
        let target = DbInstance::builder()
            .db_instance_identifier(TARGET)
            .db_instance_arn(TARGET_ARN)
            .db_instance_status("available")
            .endpoint(
                Endpoint::builder()
                    .address("app-restored.xyz.us-east-2.rds.amazonaws.com")
                    .build(),
            )
            .build();
        let rds = Rds::new(
            Some(available_source()),
            Some((target, vec![(SOURCE_TAG, SOURCE), (POINT_TAG, POINT)])),
        );
        let resp = rds.call("execute").await;
        assert_eq!(resp["outcome"], "done");
        assert_eq!(resp["result"]["submitted"], false);
        assert_eq!(
            resp["result"]["target"]["endpoint"],
            "app-restored.xyz.us-east-2.rds.amazonaws.com"
        );
        assert_eq!(rds.restore.num_calls(), 0);
    }

    #[tokio::test]
    async fn refuses_an_unrelated_target_name() {
        let target = DbInstance::builder()
            .db_instance_identifier(TARGET)
            .db_instance_arn(TARGET_ARN)
            .db_instance_status("available")
            .build();
        let rds = Rds::new(Some(available_source()), Some((target, vec![])));
        let resp = rds.call("execute").await;
        assert_eq!(resp["outcome"], "refused");
        assert_eq!(failed(&resp), ["new-server"]);
        assert_eq!(rds.restore.num_calls(), 0);
    }
}
