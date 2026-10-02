//! Grants an IAM user or role short-lived SSH access to an EC2 instance through SSM Session
//! Manager.
//!
//! Access uses Session Manager: no keys are pushed and no port is opened. Execute attaches a
//! customer-managed IAM policy that allows the principal to start a session on the instance and
//! returns the command to connect. The policy is tagged with its expiry; IAM policies don't
//! expire on their own.
//!
//! Every execute on the principal, and EventBridge every few minutes, removes expired policies
//! this function created, and nothing else. `revoke` removes the principal's access right away.
//! Re-granting never shortens an existing expiry.

use std::sync::Arc;

use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ec2::error::ProvideErrorMetadata;
use aws_sdk_ec2::types::{Instance, InstanceStateName, PlatformValues};
use aws_sdk_iam::Client as IamClient;
use aws_sdk_iam::types::{Policy, PolicyScopeType, Tag};
use aws_sdk_ssm::Client as SsmClient;
use aws_sdk_ssm::types::{InstanceInformation, PingStatus};
use functions_aws::provider_error;
use functions_core::volume::now;
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Path of the customer-managed policies this function creates.
const POLICY_PATH: &str = "/plural.sh/ssh-access/";

/// Tag that records expiry (Unix seconds). Authoritative; the description is set once at create.
const EXPIRES_TAG: &str = "plural.sh/ssh-access-expires";

/// Description prefix of the policies this function creates, followed by `expires=<unix>`.
const MARKER: &str = "plural.sh ssh-access";

const DEFAULT_DURATION_MINUTES: i64 = 60;
const MAX_DURATION_VAR: &str = "MAX_DURATION_MINUTES";
const DEFAULT_MAX_DURATION_MINUTES: i64 = 240;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    instance_id: String,
    /// ARN of the IAM user or role that gets the access.
    principal_arn: String,
    #[serde(default = "default_duration")]
    duration_minutes: i64,
    /// Remove the principal's access to the instance instead of granting it.
    #[serde(default)]
    revoke: bool,
}

fn default_duration() -> i64 {
    DEFAULT_DURATION_MINUTES
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrincipalKind {
    User,
    Role,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Principal {
    kind: PrincipalKind,
    /// 12-digit account ID from the ARN.
    account: String,
    /// Friendly name (last path segment), for Attach/Detach API calls.
    name: String,
    arn: String,
}

#[derive(Debug, Clone)]
struct InstanceView {
    id: String,
    account: String,
    state: InstanceStateName,
    /// `Some` when EC2 reports a Windows platform.
    windows: bool,
    /// SSM PingStatus, when the instance is registered with SSM.
    ssm: Option<PingStatus>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    instance_id: String,
    principal_arn: String,
    /// When access ends, for a grant.
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
    /// How to connect once access is granted.
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    granted: bool,
    /// Policies removed by this call (revoked or expired).
    removed: usize,
}

struct Aws {
    ec2: Ec2Client,
    ssm: SsmClient,
    iam: IamClient,
    region: String,
    partition: String,
}

fn validate(params: &Params) -> Result<Principal, Error> {
    if !is_instance_id(&params.instance_id) {
        return Err(Error::invalid_request(format!(
            "instanceId {:?} is not an EC2 instance ID (i-[0-9a-f]{{8,17}})",
            params.instance_id
        )));
    }
    parse_principal(&params.principal_arn).ok_or_else(|| {
        Error::invalid_request(format!(
            "principalArn {:?} is not an IAM user or role ARN (arn:aws:iam::<account>:user|role/<name>)",
            params.principal_arn
        ))
    })
}

/// `i-` followed by 8–17 hex digits.
fn is_instance_id(id: &str) -> bool {
    id.strip_prefix("i-").is_some_and(|hex| {
        (8..=17).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit())
    })
}

/// IAM user or role ARN in this account's partition family.
fn parse_principal(arn: &str) -> Option<Principal> {
    let parts: Vec<&str> = arn.splitn(6, ':').collect();
    let [prefix, partition, service, empty_region, account, resource] = parts.as_slice() else {
        return None;
    };
    if *prefix != "arn"
        || !partition.starts_with("aws")
        || *service != "iam"
        || !empty_region.is_empty()
        || account.len() != 12
        || !account.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let (kind, rest) = if let Some(rest) = resource.strip_prefix("user/") {
        (PrincipalKind::User, rest)
    } else {
        let rest = resource.strip_prefix("role/")?;
        (PrincipalKind::Role, rest)
    };
    let name = rest.rsplit('/').next().filter(|n| !n.is_empty())?;
    if name.len() > 64
        || !name.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '+' | '=' | ',' | '.' | '@' | '-' | '_')
        })
    {
        return None;
    }
    Some(Principal {
        kind,
        account: (*account).to_owned(),
        name: name.to_owned(),
        arn: arn.to_owned(),
    })
}

async fn handle(aws: &Aws, req: Request<Params>) -> Result<Response<Output>, Error> {
    let params = req.params;
    let principal = validate(&params)?;
    let now = now();
    let mut expires = effective_expires(now, params.duration_minutes, None);

    let instance = load_instance(aws, &params.instance_id).await?;
    let guards = if params.revoke {
        vec![
            match &instance {
                Some(i) => Guard::pass("exists", format!("instance {}", i.id)),
                None => Guard::fail("exists", "instance not found"),
            },
            principal_guard(&principal, instance.as_ref()),
        ]
    } else {
        evaluate(
            instance.as_ref(),
            &principal,
            params.duration_minutes,
            max_duration(std::env::var(MAX_DURATION_VAR).ok().as_deref()),
        )
    };

    let mut output = Output {
        instance_id: params.instance_id.clone(),
        principal_arn: principal.arn.clone(),
        expires_at: (!params.revoke && guards.iter().all(|g| g.passed))
            .then(|| format_time(expires)),
        command: (!params.revoke).then(|| connect_command(&params.instance_id)),
        granted: false,
        removed: 0,
    };

    match req.action {
        Action::Plan => return Ok(Response::planned(guards, output)),
        Action::Execute if !guards.iter().all(|g| g.passed) => {
            return Ok(Response::refused(guards).with_result(output));
        }
        Action::Execute => {}
    }

    // Remove expired policies for this principal, and expired ones for this instance.
    output.removed += cleanup_expired(aws, &principal, &params.instance_id, now, false).await?;

    if params.revoke {
        output.removed += revoke_ours(aws, &principal, &params.instance_id).await?;
        return Ok(Response::done(guards, output));
    }

    let Some(instance) = instance else {
        return Ok(Response::refused(guards).with_result(output));
    };

    let name = policy_name(&params.instance_id, &principal.arn);
    let existing = find_policy(aws, &name).await?;
    let existing_expires = match &existing {
        Some(policy) => policy_expires_at(aws, policy.arn().unwrap_or_default()).await?,
        None => None,
    };
    expires = effective_expires(now, params.duration_minutes, existing_expires);

    ensure_grant(
        aws,
        &instance,
        &principal,
        &name,
        existing.as_ref(),
        expires,
    )
    .await?;
    output.expires_at = Some(format_time(expires));
    tracing::info!(
        instance_id = %params.instance_id,
        principal = %principal.arn,
        expires,
        "granted ssh access"
    );
    output.granted = true;
    Ok(Response::done(guards, output))
}

fn evaluate(
    instance: Option<&InstanceView>,
    principal: &Principal,
    minutes: i64,
    max: i64,
) -> Vec<Guard> {
    let Some(instance) = instance else {
        return vec![Guard::fail("exists", "instance not found")];
    };
    let running = instance.state == InstanceStateName::Running;
    let ssm_online = instance.ssm.as_ref() == Some(&PingStatus::Online);
    vec![
        Guard::pass("exists", format!("instance {}", instance.id)),
        Guard::check(
            "running",
            running,
            format!("state {}", instance.state.as_str()),
        ),
        Guard::check(
            "linux",
            !instance.windows,
            if instance.windows {
                "platform Windows"
            } else {
                "platform linux"
            },
        ),
        Guard::check(
            "ssm-online",
            ssm_online,
            match instance.ssm.as_ref() {
                Some(status) => format!("SSM ping status {}", status.as_str()),
                None => "instance is not registered with SSM".into(),
            },
        ),
        Guard::check(
            "duration",
            (1..=max).contains(&minutes),
            format!("{minutes} minutes; at most {max} allowed"),
        ),
        principal_guard(principal, Some(instance)),
    ]
}

fn principal_guard(principal: &Principal, instance: Option<&InstanceView>) -> Guard {
    match instance {
        Some(instance) if principal.account == instance.account => Guard::pass(
            "principal",
            format!(
                "{} {} in account {}",
                match principal.kind {
                    PrincipalKind::User => "user",
                    PrincipalKind::Role => "role",
                },
                principal.name,
                principal.account
            ),
        ),
        Some(instance) => Guard::fail(
            "principal",
            format!(
                "principal account {} does not match instance account {}",
                principal.account, instance.account
            ),
        ),
        // Without an instance the account can't be checked; revoke still needs a valid ARN.
        None => Guard::pass(
            "principal",
            format!(
                "{} {}",
                match principal.kind {
                    PrincipalKind::User => "user",
                    PrincipalKind::Role => "role",
                },
                principal.name
            ),
        ),
    }
}

async fn load_instance(aws: &Aws, id: &str) -> Result<Option<InstanceView>, Error> {
    let out = match aws.ec2.describe_instances().instance_ids(id).send().await {
        Ok(out) => out,
        Err(err)
            if matches!(
                err.code(),
                Some("InvalidInstanceID.NotFound" | "InvalidInstanceID.Malformed")
            ) =>
        {
            return Ok(None);
        }
        Err(err) => return Err(provider_error(err)),
    };
    let Some((account, inst)) = out.reservations().iter().find_map(|r| {
        r.instances()
            .first()
            .map(|inst| (r.owner_id().unwrap_or_default().to_owned(), inst))
    }) else {
        return Ok(None);
    };
    let ssm = ssm_info(aws, id).await?;
    Ok(Some(instance_from(inst, account, ssm)))
}

fn instance_from(
    inst: &Instance,
    account: String,
    ssm: Option<InstanceInformation>,
) -> InstanceView {
    InstanceView {
        id: inst.instance_id().unwrap_or_default().to_owned(),
        account,
        state: inst
            .state()
            .and_then(|s| s.name())
            .cloned()
            .unwrap_or(InstanceStateName::Pending),
        windows: matches!(inst.platform(), Some(PlatformValues::Windows)),
        ssm: ssm.and_then(|i| i.ping_status().cloned()),
    }
}

async fn ssm_info(aws: &Aws, id: &str) -> Result<Option<InstanceInformation>, Error> {
    let out = aws
        .ssm
        .describe_instance_information()
        .filters(
            aws_sdk_ssm::types::InstanceInformationStringFilter::builder()
                .key("InstanceIds")
                .values(id)
                .build()
                .map_err(|err| Error::provider(err.to_string()))?,
        )
        .send()
        .await
        .map_err(provider_error)?;
    Ok(out.instance_information_list().first().cloned())
}

/// Stable policy name for this instance and principal.
fn policy_name(instance_id: &str, principal_arn: &str) -> String {
    format!("ssh-{instance_id}-{:08x}", principal_hash(principal_arn))
}

/// FNV-1a 32-bit of the principal ARN; stable across Rust versions.
fn principal_hash(arn: &str) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for b in arn.bytes() {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

fn policy_document(aws: &Aws, instance: &InstanceView) -> Result<String, Error> {
    let instance_arn = format!(
        "arn:{}:ec2:{}:{}:instance/{}",
        aws.partition, aws.region, instance.account, instance.id
    );
    let shell_doc = format!(
        "arn:{}:ssm:{}::document/SSM-SessionManagerRunShell",
        aws.partition, aws.region
    );
    let ssh_doc = format!(
        "arn:{}:ssm:{}::document/AWS-StartSSHSession",
        aws.partition, aws.region
    );
    let session_arn = format!("arn:{}:ssm:*:*:session/${{aws:userid}}-*", aws.partition);
    serde_json::to_string(&json!({
        "Version": "2012-10-17",
        "Statement": [
            {
                "Effect": "Allow",
                "Action": "ssm:StartSession",
                "Resource": [instance_arn, shell_doc, ssh_doc],
            },
            {
                "Effect": "Allow",
                "Action": ["ssm:TerminateSession", "ssm:ResumeSession"],
                "Resource": session_arn,
            },
        ],
    }))
    .map_err(Error::provider)
}

async fn find_policy(aws: &Aws, name: &str) -> Result<Option<Policy>, Error> {
    let mut marker = None;
    loop {
        let out = aws
            .iam
            .list_policies()
            .scope(PolicyScopeType::Local)
            .path_prefix(POLICY_PATH)
            .set_marker(marker)
            .send()
            .await
            .map_err(provider_error)?;
        if let Some(policy) = out
            .policies()
            .iter()
            .find(|p| p.policy_name() == Some(name))
        {
            return Ok(Some(policy.clone()));
        }
        if !out.is_truncated() {
            return Ok(None);
        }
        marker = out.marker().map(str::to_owned);
    }
}

async fn policy_expires_at(aws: &Aws, policy_arn: &str) -> Result<Option<i64>, Error> {
    let out = aws
        .iam
        .list_policy_tags()
        .policy_arn(policy_arn)
        .send()
        .await
        .map_err(provider_error)?;
    Ok(out
        .tags()
        .iter()
        .find(|t| t.key() == EXPIRES_TAG)
        .map(|t| t.value())
        .and_then(|v| v.parse().ok()))
}

async fn ensure_grant(
    aws: &Aws,
    instance: &InstanceView,
    principal: &Principal,
    name: &str,
    existing: Option<&Policy>,
    expires: i64,
) -> Result<(), Error> {
    let arn = match existing {
        Some(policy) => {
            let arn = policy.arn().unwrap_or_default().to_owned();
            tag_expires(aws, &arn, expires).await?;
            arn
        }
        None => match create_policy(aws, instance, name, expires).await {
            Ok(arn) => arn,
            // A concurrent grant created it first: tag and attach that one.
            Err(err) if is_entity_already_exists(&err) => {
                let policy = find_policy(aws, name).await?.ok_or(err)?;
                let arn = policy.arn().unwrap_or_default().to_owned();
                tag_expires(aws, &arn, expires).await?;
                arn
            }
            Err(err) => return Err(err),
        },
    };
    attach_policy(aws, principal, &arn).await
}

async fn create_policy(
    aws: &Aws,
    instance: &InstanceView,
    name: &str,
    expires: i64,
) -> Result<String, Error> {
    let document = policy_document(aws, instance)?;
    let out = aws
        .iam
        .create_policy()
        .policy_name(name)
        .path(POLICY_PATH)
        .policy_document(document)
        .description(description(expires))
        .tags(
            Tag::builder()
                .key(EXPIRES_TAG)
                .value(expires.to_string())
                .build()
                .map_err(|err| Error::provider(err.to_string()))?,
        )
        .send()
        .await
        .map_err(provider_error)?;
    out.policy()
        .and_then(|p| p.arn())
        .map(str::to_owned)
        .ok_or_else(|| Error::provider("CreatePolicy returned no policy ARN"))
}

fn is_entity_already_exists(err: &Error) -> bool {
    matches!(err, Error::Provider(msg) if msg.starts_with("EntityAlreadyExists:"))
}

async fn tag_expires(aws: &Aws, policy_arn: &str, expires: i64) -> Result<(), Error> {
    aws.iam
        .tag_policy()
        .policy_arn(policy_arn)
        .tags(
            Tag::builder()
                .key(EXPIRES_TAG)
                .value(expires.to_string())
                .build()
                .map_err(|err| Error::provider(err.to_string()))?,
        )
        .send()
        .await
        .map_err(provider_error)?;
    Ok(())
}

async fn attach_policy(aws: &Aws, principal: &Principal, policy_arn: &str) -> Result<(), Error> {
    match principal.kind {
        PrincipalKind::User => {
            aws.iam
                .attach_user_policy()
                .user_name(&principal.name)
                .policy_arn(policy_arn)
                .send()
                .await
                .map_err(provider_error)?;
        }
        PrincipalKind::Role => {
            aws.iam
                .attach_role_policy()
                .role_name(&principal.name)
                .policy_arn(policy_arn)
                .send()
                .await
                .map_err(provider_error)?;
        }
    }
    Ok(())
}

/// Detach and delete the policy for this principal and instance, if it exists.
async fn revoke_ours(aws: &Aws, principal: &Principal, instance_id: &str) -> Result<usize, Error> {
    let name = policy_name(instance_id, &principal.arn);
    let Some(policy) = find_policy(aws, &name).await? else {
        return Ok(0);
    };
    let arn = policy.arn().unwrap_or_default();
    delete_policy(aws, arn).await?;
    tracing::info!(policy = %arn, principal = %principal.arn, "removed ssh access");
    Ok(1)
}

/// Removes expired policies under our path that are attached to `principal`, or that grant this
/// instance. When `all_principals` is set (the timer), every expired policy is removed.
async fn cleanup_expired(
    aws: &Aws,
    principal: &Principal,
    instance_id: &str,
    now: i64,
    all_principals: bool,
) -> Result<usize, Error> {
    let policies = list_our_policies(aws).await?;
    let mut removed = 0;
    for policy in policies {
        let arn = policy.arn().unwrap_or_default();
        let name = policy.policy_name().unwrap_or_default();
        let Some(expires) = policy_expires_at(aws, arn).await? else {
            continue;
        };
        if expires > now {
            continue;
        }
        let ours_for_instance = name.starts_with(&format!("ssh-{instance_id}-"));
        let take =
            all_principals || ours_for_instance || policy_attached_to(aws, arn, principal).await?;
        if !take {
            continue;
        }
        match delete_policy(aws, arn).await {
            Ok(()) => {
                tracing::info!(policy = %arn, "removed expired ssh access");
                removed += 1;
            }
            Err(err) if !all_principals && ours_for_instance => return Err(err),
            Err(err) => {
                tracing::warn!(policy = %arn, error = %err, "couldn't remove expired ssh access");
            }
        }
    }
    Ok(removed)
}

async fn policy_attached_to(
    aws: &Aws,
    policy_arn: &str,
    principal: &Principal,
) -> Result<bool, Error> {
    let out = aws
        .iam
        .list_entities_for_policy()
        .policy_arn(policy_arn)
        .send()
        .await
        .map_err(provider_error)?;
    Ok(match principal.kind {
        PrincipalKind::User => out
            .policy_users()
            .iter()
            .any(|u| u.user_name() == Some(principal.name.as_str())),
        PrincipalKind::Role => out
            .policy_roles()
            .iter()
            .any(|r| r.role_name() == Some(principal.name.as_str())),
    })
}

async fn list_our_policies(aws: &Aws) -> Result<Vec<Policy>, Error> {
    let mut policies = Vec::new();
    let mut marker = None;
    loop {
        let out = aws
            .iam
            .list_policies()
            .scope(PolicyScopeType::Local)
            .path_prefix(POLICY_PATH)
            .set_marker(marker)
            .send()
            .await
            .map_err(provider_error)?;
        policies.extend(out.policies().iter().cloned());
        if !out.is_truncated() {
            return Ok(policies);
        }
        marker = out.marker().map(str::to_owned);
    }
}

/// Detach from every attached entity, delete non-default versions, then delete the policy.
async fn delete_policy(aws: &Aws, policy_arn: &str) -> Result<(), Error> {
    let entities = aws
        .iam
        .list_entities_for_policy()
        .policy_arn(policy_arn)
        .send()
        .await
        .map_err(provider_error)?;
    for user in entities.policy_users() {
        if let Some(name) = user.user_name() {
            aws.iam
                .detach_user_policy()
                .user_name(name)
                .policy_arn(policy_arn)
                .send()
                .await
                .map_err(provider_error)?;
        }
    }
    for role in entities.policy_roles() {
        if let Some(name) = role.role_name() {
            aws.iam
                .detach_role_policy()
                .role_name(name)
                .policy_arn(policy_arn)
                .send()
                .await
                .map_err(provider_error)?;
        }
    }
    let versions = aws
        .iam
        .list_policy_versions()
        .policy_arn(policy_arn)
        .send()
        .await
        .map_err(provider_error)?;
    for version in versions.versions() {
        if version.is_default_version() {
            continue;
        }
        if let Some(id) = version.version_id() {
            aws.iam
                .delete_policy_version()
                .policy_arn(policy_arn)
                .version_id(id)
                .send()
                .await
                .map_err(provider_error)?;
        }
    }
    aws.iam
        .delete_policy()
        .policy_arn(policy_arn)
        .send()
        .await
        .map_err(provider_error)?;
    Ok(())
}

/// Removes every expired policy this function created.
async fn expire(aws: &Aws) -> Result<(), Error> {
    let now = now();
    let policies = list_our_policies(aws).await?;
    let mut failures = Vec::new();
    for policy in policies {
        let arn = policy.arn().unwrap_or_default();
        let Some(expires) = policy_expires_at(aws, arn).await.unwrap_or_else(|err| {
            failures.push(format!("{arn}: {err}"));
            None
        }) else {
            continue;
        };
        if expires > now {
            continue;
        }
        match delete_policy(aws, arn).await {
            Ok(()) => tracing::info!(policy = %arn, "removed expired ssh access"),
            Err(err) => failures.push(format!("{arn}: {err}")),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::provider(format!(
            "couldn't remove expired ssh access: {}",
            failures.join("; ")
        )))
    }
}

fn max_duration(value: Option<&str>) -> i64 {
    value
        .and_then(|v| v.parse().ok())
        .filter(|m: &i64| *m > 0)
        .unwrap_or(DEFAULT_MAX_DURATION_MINUTES)
}

fn description(expires: i64) -> String {
    format!("{MARKER} expires={expires} ({})", format_time(expires))
}

fn format_time(unix: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(unix)
        .ok()
        .and_then(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| unix.to_string())
}

fn connect_command(instance_id: &str) -> String {
    format!("aws ssm start-session --target {instance_id}")
}

fn partition_for(region: &str) -> &'static str {
    if region.starts_with("cn-") {
        "aws-cn"
    } else if region.starts_with("us-gov-") {
        "aws-us-gov"
    } else {
        "aws"
    }
}

/// Proposed expiry, never shortened below an existing unexpired tag.
fn effective_expires(now: i64, duration_minutes: i64, existing_tag: Option<i64>) -> i64 {
    let proposed = now.saturating_add(duration_minutes.saturating_mul(60));
    proposed.max(existing_tag.filter(|e| *e > now).unwrap_or(0))
}

#[tokio::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    let config = functions_aws::sdk_config().await;
    let region = config
        .region()
        .map(|r| r.to_string())
        .unwrap_or_else(|| "us-east-1".into());
    let aws = Arc::new(Aws {
        ec2: Ec2Client::new(&config),
        ssm: SsmClient::new(&config),
        iam: IamClient::new(&config),
        partition: partition_for(&region).to_owned(),
        region,
    });

    functions_aws::run_with_expire(
        {
            let aws = Arc::clone(&aws);
            move |req| {
                let aws = Arc::clone(&aws);
                async move { handle(&aws, req).await }
            }
        },
        {
            let aws = Arc::clone(&aws);
            move || {
                let aws = Arc::clone(&aws);
                async move { expire(&aws).await }
            }
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(guards: &[Guard]) -> Vec<&str> {
        guards
            .iter()
            .filter(|g| !g.passed)
            .map(|g| g.name.as_str())
            .collect()
    }

    fn instance(windows: bool, state: InstanceStateName, ssm: Option<PingStatus>) -> InstanceView {
        InstanceView {
            id: "i-0123456789abcdef0".into(),
            account: "123456789012".into(),
            state,
            windows,
            ssm,
        }
    }

    fn user() -> Principal {
        parse_principal("arn:aws:iam::123456789012:user/alice").unwrap()
    }

    #[test]
    fn validates_instance_ids_and_principal_arns() {
        assert!(is_instance_id("i-0123456789abcdef0"));
        assert!(is_instance_id("i-01234567"));
        assert!(!is_instance_id("i-0123456"));
        assert!(!is_instance_id("i-0123456789abcdef01")); // 18 hex
        assert!(!is_instance_id("i-0123456789abcdefg"));
        assert!(!is_instance_id("ami-0123456789abcdef0"));

        assert!(parse_principal("arn:aws:iam::123456789012:user/alice").is_some());
        assert!(parse_principal("arn:aws:iam::123456789012:role/path/MyRole").is_some());
        assert_eq!(
            parse_principal("arn:aws:iam::123456789012:role/path/MyRole")
                .unwrap()
                .name,
            "MyRole"
        );
        assert!(parse_principal("arn:aws:iam::123456789012:root").is_none());
        assert!(parse_principal("arn:aws:iam::123456789012:assumed-role/x/y").is_none());
        assert!(parse_principal("arn:aws:sts::123456789012:assumed-role/x/y").is_none());
        assert!(parse_principal("arn:aws:iam::12345:user/alice").is_none());
    }

    #[test]
    fn grants_to_running_linux_instances_with_ssm() {
        let ok = instance(false, InstanceStateName::Running, Some(PingStatus::Online));
        let principal = user();

        assert!(failed(&evaluate(Some(&ok), &principal, 60, 240)).is_empty());
        assert_eq!(
            failed(&evaluate(
                Some(&instance(
                    true,
                    InstanceStateName::Running,
                    Some(PingStatus::Online)
                )),
                &principal,
                60,
                240
            )),
            ["linux"]
        );
        assert_eq!(
            failed(&evaluate(
                Some(&instance(
                    false,
                    InstanceStateName::Stopped,
                    Some(PingStatus::Online)
                )),
                &principal,
                60,
                240
            )),
            ["running"]
        );
        assert_eq!(
            failed(&evaluate(
                Some(&instance(false, InstanceStateName::Running, None)),
                &principal,
                60,
                240
            )),
            ["ssm-online"]
        );
        assert_eq!(
            failed(&evaluate(
                Some(&instance(
                    false,
                    InstanceStateName::Running,
                    Some(PingStatus::ConnectionLost)
                )),
                &principal,
                60,
                240
            )),
            ["ssm-online"]
        );
        assert_eq!(
            failed(&evaluate(Some(&ok), &principal, 241, 240)),
            ["duration"]
        );
        assert_eq!(
            failed(&evaluate(Some(&ok), &principal, 0, 240)),
            ["duration"]
        );
        assert_eq!(failed(&evaluate(None, &principal, 60, 240)), ["exists"]);

        let other = Principal {
            account: "999999999999".into(),
            ..principal.clone()
        };
        assert_eq!(failed(&evaluate(Some(&ok), &other, 60, 240)), ["principal"]);
    }

    #[test]
    fn describes_the_expiry_readably() {
        assert_eq!(
            description(1_790_787_600),
            "plural.sh ssh-access expires=1790787600 (2026-09-30T17:00:00Z)"
        );
    }

    #[test]
    fn granting_again_never_shortens_access() {
        let now = 1_700_000_000;
        let later = now + 200 * 60;
        assert_eq!(effective_expires(now, 10, Some(later)), later);
        assert_eq!(
            effective_expires(now, 30, Some(now + 10 * 60)),
            now + 30 * 60
        );
        // An already-expired tag is ignored.
        assert_eq!(effective_expires(now, 60, Some(now - 60)), now + 60 * 60);
        assert_eq!(effective_expires(now, 60, None), now + 60 * 60);
    }

    #[test]
    fn policy_names_are_stable_for_the_same_principal() {
        let arn = "arn:aws:iam::123456789012:user/alice";
        assert_eq!(
            policy_name("i-0123456789abcdef0", arn),
            policy_name("i-0123456789abcdef0", arn)
        );
        assert_ne!(
            policy_name("i-0123456789abcdef0", arn),
            policy_name("i-0123456789abcdef0", "arn:aws:iam::123456789012:user/bob")
        );
        assert!(policy_name("i-0123456789abcdef0", arn).starts_with("ssh-i-0123456789abcdef0-"));
    }

    #[test]
    fn connect_command_targets_the_instance() {
        assert_eq!(
            connect_command("i-0123456789abcdef0"),
            "aws ssm start-session --target i-0123456789abcdef0"
        );
        assert_eq!(max_duration(None), DEFAULT_MAX_DURATION_MINUTES);
        assert_eq!(max_duration(Some("30")), 30);
        assert_eq!(max_duration(Some("-1")), DEFAULT_MAX_DURATION_MINUTES);
    }
}
