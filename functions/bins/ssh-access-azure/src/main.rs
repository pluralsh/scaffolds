//! Grants an Entra ID user short-lived SSH login to an Azure VM.
//!
//! Access uses Microsoft Entra ID login for Linux VMs: no keys are pushed to the VM and no
//! port is opened. Execute assigns the user the Virtual Machine User Login (or Administrator
//! Login) role on the VM and returns the command to connect, through Azure Bastion when the
//! installation configures one. Azure role assignments don't expire, so the expiry is kept in
//! the assignment's description: every execute on the VM and a timer every few minutes
//! remove the expired assignments this function created, and nothing else. `revoke` removes
//! the user's access right away.
//!
//! The function's own permission to manage role assignments is limited by a role assignment
//! condition to these two roles and to users, so it can't grant anything else.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;

use functions_azure::arm::{self, Arm, Connector, Precondition, ResourceId};
use functions_core::volume::now;
use functions_core::{Action, Error, Guard, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

const VM_API_VERSION: &str = "2024-07-01";
const AUTHORIZATION_API_VERSION: &str = "2022-04-01";

/// Built-in roles that allow Entra ID login to a VM.
const USER_LOGIN_ROLE: &str = "fb879df8-f326-4884-b1cf-06f3ad86be52";
const ADMIN_LOGIN_ROLE: &str = "1c0163c0-47e6-4577-8991-ea5c82e286e4";

/// Start of the description of the role assignments this function creates, followed by
/// `expires=<unix seconds>`. Only assignments with it are ever removed.
const MARKER: &str = "plural.sh ssh-access";

/// VM extension that enables Entra ID login on Linux.
const LOGIN_EXTENSION: &str = "AADSSHLoginForLinux";

/// The timer function in the app that removes expired assignments.
const EXPIRE_FUNCTION: &str = "ssh-access-expire";

const DEFAULT_DURATION_MINUTES: i64 = 60;
const MAX_DURATION_VAR: &str = "MAX_DURATION_MINUTES";
const DEFAULT_MAX_DURATION_MINUTES: i64 = 240;
/// Resource ID of the Bastion host users connect through, if the installation has one.
const BASTION_VAR: &str = "BASTION_ID";
/// JSON list of the resource groups the function may grant access in, swept by the timer.
const SCOPES_VAR: &str = "SCOPES";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Role {
    #[default]
    User,
    Admin,
}

impl Role {
    fn definition(self) -> &'static str {
        match self {
            Self::User => USER_LOGIN_ROLE,
            Self::Admin => ADMIN_LOGIN_ROLE,
        }
    }

    fn of(definition_id: &str) -> Option<Self> {
        let guid = definition_id.rsplit('/').next()?;
        [Self::User, Self::Admin]
            .into_iter()
            .find(|r| guid.eq_ignore_ascii_case(r.definition()))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    vm_id: String,
    /// Object ID of the Entra ID user.
    principal_id: String,
    #[serde(default)]
    role: Role,
    #[serde(default = "default_duration")]
    duration_minutes: i64,
    /// Remove the user's access to the VM instead of granting it.
    #[serde(default)]
    revoke: bool,
}

fn default_duration() -> i64 {
    DEFAULT_DURATION_MINUTES
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Vm {
    #[serde(default)]
    name: String,
    #[serde(default)]
    properties: VmProperties,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VmProperties {
    #[serde(default)]
    storage_profile: StorageProfile,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StorageProfile {
    #[serde(default)]
    os_disk: Option<OsDisk>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OsDisk {
    #[serde(default)]
    os_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Extension {
    #[serde(default)]
    properties: ExtensionProperties,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionProperties {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    provisioning_state: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Assignment {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    properties: AssignmentProperties,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignmentProperties {
    #[serde(default)]
    role_definition_id: String,
    #[serde(default)]
    principal_id: String,
    #[serde(default)]
    principal_type: Option<String>,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    description: Option<String>,
}

impl Assignment {
    /// Expiry (Unix seconds) of an assignment this function created, `None` for any other.
    fn expires_at(&self) -> Option<i64> {
        let props = &self.properties;
        Role::of(&props.role_definition_id)?;
        if !props
            .principal_type
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case("User"))
        {
            return None;
        }
        let rest = props.description.as_deref()?.strip_prefix(MARKER)?;
        let expires = rest.trim_start().strip_prefix("expires=")?;
        let digits: String = expires.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    vm: String,
    principal_id: String,
    role: Role,
    /// When access ends, for a grant.
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
    /// How to connect once access is granted.
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    /// Scopes of the user's other VM login role assignments, which this function neither
    /// grants nor revokes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    other_access: Vec<String>,
    granted: bool,
    /// Role assignments removed by this call (revoked or expired).
    removed: usize,
}

async fn handle(connector: &Connector, req: Request<Params>) -> Result<Response<Output>, Error> {
    let params = req.params;
    let vm_id = ResourceId::parse(&params.vm_id, &[arm::VIRTUAL_MACHINE]).ok_or_else(|| {
        Error::invalid_request(format!(
            "vmId {:?} is not a virtual machine resource ID (/subscriptions/<id>/resourceGroups/<group>/providers/Microsoft.Compute/virtualMachines/<name>)",
            params.vm_id
        ))
    })?;
    if !arm::is_guid(&params.principal_id) {
        return Err(Error::invalid_request(format!(
            "principalId {:?} is not an Entra ID object ID",
            params.principal_id
        )));
    }
    let principal = params.principal_id.to_lowercase();
    let now = now();
    // Out-of-range durations are refused by the duration guard; this only keeps them finite.
    let mut expires = now.saturating_add(params.duration_minutes.saturating_mul(60));

    let arm = connector.connect().await?;
    let vm: Option<Vm> = arm.get(&vm_id.id(), VM_API_VERSION).await?;
    let (extensions, assignments): (Vec<Extension>, Vec<Assignment>) = match vm {
        Some(_) => (
            arm.list(&format!("{}/extensions", vm_id.id()), VM_API_VERSION, None)
                .await?,
            assignments_at(&arm, &vm_id.id()).await?,
        ),
        None => (vec![], vec![]),
    };
    let ours: Vec<&Assignment> = assignments
        .iter()
        .filter(|a| {
            a.expires_at().is_some() && a.properties.scope.eq_ignore_ascii_case(&vm_id.id())
        })
        .collect();
    // Login access of the user that this function didn't grant, which revoke doesn't remove.
    let vm_path = vm_id.id().to_lowercase();
    let other_access: Vec<String> = assignments
        .iter()
        .filter(|a| {
            let scope = a.properties.scope.to_lowercase();
            a.expires_at().is_none()
                && Role::of(&a.properties.role_definition_id).is_some()
                && a.properties.principal_id.eq_ignore_ascii_case(&principal)
                && (vm_path == scope || vm_path.starts_with(&format!("{scope}/")))
        })
        .map(|a| a.properties.scope.clone())
        .collect();

    let guards = if params.revoke {
        vec![match vm {
            Some(_) => Guard::pass("exists", format!("VM {}", vm_id.name)),
            None => Guard::fail("exists", "VM not found"),
        }]
    } else {
        evaluate(
            vm.as_ref(),
            &extensions,
            params.duration_minutes,
            max_duration(std::env::var(MAX_DURATION_VAR).ok().as_deref()),
        )
    };
    let mut output = Output {
        vm: vm_id.id(),
        principal_id: principal.clone(),
        role: params.role,
        expires_at: (!params.revoke).then(|| format_time(expires)),
        command: if params.revoke {
            None
        } else {
            let bastion =
                bastion(std::env::var(BASTION_VAR).ok().as_deref()).map_err(Error::provider)?;
            Some(connect_command(&vm_id, bastion.as_ref()))
        },
        other_access,
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

    // The user's assignments when revoking, which must all go, and expired ones of anyone,
    // which the timer would remove anyway.
    for assignment in &ours {
        let own = assignment
            .properties
            .principal_id
            .eq_ignore_ascii_case(&principal);
        let expired = assignment.expires_at().is_some_and(|e| e <= now);
        if !(expired || (params.revoke && own)) {
            continue;
        }
        let deleted = arm
            .delete(
                &assignment_path(&assignment.properties.scope, &assignment.name),
                AUTHORIZATION_API_VERSION,
                Precondition::Always,
            )
            .await;
        match deleted {
            Ok(()) => {
                tracing::info!(vm = %vm_id.id(), principal = %assignment.properties.principal_id, "removed ssh access");
                output.removed += 1;
            }
            Err(err) if !(params.revoke && own) => {
                tracing::warn!(assignment = %assignment.id, error = %err, "couldn't remove expired ssh access");
            }
            Err(err) => return Err(err),
        }
    }
    if params.revoke {
        return Ok(Response::done(guards, output));
    }

    // A grant for the same user and role updates the existing assignment, and never shortens it.
    let existing = ours
        .iter()
        .copied()
        .find(|a| is_grant(a, &principal, params.role, now));
    let name = existing.map_or_else(new_guid, |a| a.name.clone());
    expires = expires.max(existing.and_then(Assignment::expires_at).unwrap_or(0));
    let granted = put_grant(&arm, &vm_id, &name, &principal, params.role, expires).await;
    match granted {
        // A concurrent grant created the user's assignment first: extend that one instead.
        Err(Error::Provider(msg))
            if existing.is_none() && msg.starts_with("RoleAssignmentExists:") =>
        {
            let assignments = assignments_at(&arm, &vm_id.id()).await?;
            let Some(current) = assignments.iter().find(|a| {
                a.properties.scope.eq_ignore_ascii_case(&vm_id.id())
                    && is_grant(a, &principal, params.role, now)
            }) else {
                return Err(Error::Provider(msg));
            };
            expires = expires.max(current.expires_at().unwrap_or(0));
            put_grant(
                &arm,
                &vm_id,
                &current.name,
                &principal,
                params.role,
                expires,
            )
            .await?;
        }
        result => result?,
    }
    output.expires_at = Some(format_time(expires));
    tracing::info!(vm = %vm_id.id(), principal = %principal, role = ?params.role, expires, "granted ssh access");
    output.granted = true;
    Ok(Response::done(guards, output))
}

/// Whether `assignment` is an unexpired grant of `role` to `principal` by this function.
fn is_grant(assignment: &Assignment, principal: &str, role: Role, now: i64) -> bool {
    assignment
        .properties
        .principal_id
        .eq_ignore_ascii_case(principal)
        && Role::of(&assignment.properties.role_definition_id) == Some(role)
        && assignment.expires_at().is_some_and(|e| e > now)
}

/// Creates or updates the role assignment `name` on the VM, granting `role` until `expires`.
async fn put_grant(
    arm: &Arm,
    vm_id: &ResourceId,
    name: &str,
    principal: &str,
    role: Role,
    expires: i64,
) -> Result<(), Error> {
    let body = json!({
        "properties": {
            "roleDefinitionId": format!(
                "/subscriptions/{}/providers/Microsoft.Authorization/roleDefinitions/{}",
                vm_id.subscription,
                role.definition()
            ),
            "principalId": principal,
            "principalType": "User",
            "description": description(expires),
        }
    });
    arm.put(
        &assignment_path(&vm_id.id(), name),
        AUTHORIZATION_API_VERSION,
        &body,
        Precondition::Always,
    )
    .await
}

/// Removes the expired role assignments this function created in its resource groups.
async fn expire(connector: &Connector, scopes: &[String]) -> Result<(), Error> {
    let arm = connector.connect().await?;
    let now = now();
    // A failure in one scope or assignment must not keep the others from expiring.
    let mut failures = Vec::new();
    for scope in scopes {
        let assignments = match assignments_at(&arm, scope).await {
            Ok(assignments) => assignments,
            Err(err) => {
                failures.push(format!("{scope}: {err}"));
                continue;
            }
        };
        for assignment in assignments {
            let under_scope = assignment
                .properties
                .scope
                .to_lowercase()
                .starts_with(&format!("{}/", scope.to_lowercase()));
            if !under_scope || !assignment.expires_at().is_some_and(|e| e <= now) {
                continue;
            }
            let path = assignment_path(&assignment.properties.scope, &assignment.name);
            match arm
                .delete(&path, AUTHORIZATION_API_VERSION, Precondition::Always)
                .await
            {
                Ok(()) => tracing::info!(assignment = %assignment.id, "removed expired ssh access"),
                Err(err) => failures.push(format!("{}: {err}", assignment.id)),
            }
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

/// Role assignments at `scope` and below it.
async fn assignments_at(arm: &Arm, scope: &str) -> Result<Vec<Assignment>, Error> {
    arm.list(
        &format!("{scope}/providers/Microsoft.Authorization/roleAssignments"),
        AUTHORIZATION_API_VERSION,
        None,
    )
    .await
}

fn assignment_path(scope: &str, name: &str) -> String {
    format!("{scope}/providers/Microsoft.Authorization/roleAssignments/{name}")
}

fn evaluate(vm: Option<&Vm>, extensions: &[Extension], minutes: i64, max: i64) -> Vec<Guard> {
    let Some(vm) = vm else {
        return vec![Guard::fail("exists", "VM not found")];
    };
    let os = vm
        .properties
        .storage_profile
        .os_disk
        .as_ref()
        .and_then(|d| d.os_type.as_deref())
        .unwrap_or("unknown");
    let login = extensions
        .iter()
        .find(|e| e.properties.kind.eq_ignore_ascii_case(LOGIN_EXTENSION));
    vec![
        Guard::pass("exists", format!("VM {}", vm.name)),
        Guard::check(
            "linux",
            os.eq_ignore_ascii_case("Linux"),
            format!("OS {os}"),
        ),
        match login {
            Some(ext) if ext.properties.provisioning_state == "Succeeded" => {
                Guard::pass("entra-login", format!("{LOGIN_EXTENSION} installed"))
            }
            Some(ext) => Guard::fail(
                "entra-login",
                format!("{LOGIN_EXTENSION} is {}", ext.properties.provisioning_state),
            ),
            None => Guard::fail(
                "entra-login",
                format!("the VM needs the {LOGIN_EXTENSION} extension for Entra ID login"),
            ),
        },
        Guard::check(
            "duration",
            (1..=max).contains(&minutes),
            format!("{minutes} minutes; at most {max} allowed"),
        ),
    ]
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

/// The Bastion host users connect through, from `BASTION_ID`. A value that isn't a Bastion
/// host ID is an error rather than no Bastion host, so a typo doesn't hand out a direct
/// connection command.
fn bastion(value: Option<&str>) -> Result<Option<ResourceId>, String> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => ResourceId::parse(v, &[arm::BASTION_HOST])
            .map(Some)
            .ok_or_else(|| format!("{BASTION_VAR} {v:?} isn't a Bastion host resource ID")),
    }
}

/// Resource groups the timer removes expired access in, from `SCOPES`. A trailing slash is
/// dropped so that assignments below the scope still match it.
fn scopes(value: Option<&str>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(vec![]);
    };
    let scopes: Vec<String> =
        serde_json::from_str(value).map_err(|err| format!("{SCOPES_VAR}: {err}"))?;
    scopes
        .into_iter()
        .map(|scope| {
            let trimmed = scope.trim().trim_end_matches('/');
            if trimmed.is_empty() {
                Err(format!("{SCOPES_VAR}: empty scope {scope:?}"))
            } else {
                Ok(trimmed.to_owned())
            }
        })
        .collect()
}

/// Command to connect: through the Bastion host if there is one, directly otherwise.
fn connect_command(vm: &ResourceId, bastion: Option<&ResourceId>) -> String {
    match bastion {
        Some(b) => format!(
            "az network bastion ssh --subscription {} --resource-group {} --name {} --target-resource-id {} --auth-type AAD",
            b.subscription,
            b.resource_group,
            b.name,
            vm.id()
        ),
        None => format!("az ssh vm --ids {}", vm.id()),
    }
}

/// A random role assignment name (GUID).
fn new_guid() -> String {
    let random = |salt: u64| {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_i64(now());
        h.finish()
    };
    let (a, b) = (random(1), random(2));
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0x0fff,
        ((b >> 48) & 0x3fff) | 0x8000,
        b & 0xffff_ffff_ffff
    )
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let client = functions_http::https_client().map_err(std::io::Error::other)?;
    let connector = Connector::from_env(client).map_err(std::io::Error::other)?;
    let timer_connector = connector.clone();
    let scopes =
        Arc::new(scopes(std::env::var(SCOPES_VAR).ok().as_deref()).map_err(std::io::Error::other)?);
    // A misconfigured Bastion host fails the start instead of every grant.
    bastion(std::env::var(BASTION_VAR).ok().as_deref()).map_err(std::io::Error::other)?;

    functions_http::run_with_timer(
        move |req| {
            let connector = connector.clone();
            async move { handle(&connector, req).await }
        },
        EXPIRE_FUNCTION,
        move || {
            let connector = timer_connector.clone();
            let scopes = Arc::clone(&scopes);
            async move { expire(&connector, &scopes).await }
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUB: &str = "00000000-0000-0000-0000-000000000000";

    fn vm_id() -> ResourceId {
        ResourceId::parse(
            &format!("/subscriptions/{SUB}/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/vm-1"),
            &[arm::VIRTUAL_MACHINE],
        )
        .unwrap()
    }

    fn vm(os: &str) -> Vm {
        serde_json::from_value(
            json!({"name": "vm-1", "properties": {"storageProfile": {"osDisk": {"osType": os}}}}),
        )
        .unwrap()
    }

    fn extension(kind: &str, state: &str) -> Extension {
        serde_json::from_value(json!({"properties": {"type": kind, "provisioningState": state}}))
            .unwrap()
    }

    fn assignment(role: &str, principal_type: &str, description: Option<&str>) -> Assignment {
        serde_json::from_value(json!({
            "id": "i", "name": "n",
            "properties": {
                "roleDefinitionId": format!("/subscriptions/{SUB}/providers/Microsoft.Authorization/roleDefinitions/{role}"),
                "principalId": "p", "principalType": principal_type, "scope": vm_id().id(),
                "description": description,
            }
        }))
        .unwrap()
    }

    fn failed(guards: &[Guard]) -> Vec<&str> {
        guards
            .iter()
            .filter(|g| !g.passed)
            .map(|g| g.name.as_str())
            .collect()
    }

    #[test]
    fn grants_to_linux_vms_with_entra_login() {
        let ok = [extension("AADSSHLoginForLinux", "Succeeded")];

        assert!(failed(&evaluate(Some(&vm("Linux")), &ok, 60, 240)).is_empty());
        assert_eq!(
            failed(&evaluate(Some(&vm("Windows")), &ok, 60, 240)),
            ["linux"]
        );
        assert_eq!(
            failed(&evaluate(Some(&vm("Linux")), &[], 60, 240)),
            ["entra-login"]
        );
        assert_eq!(
            failed(&evaluate(
                Some(&vm("Linux")),
                &[extension("AADSSHLoginForLinux", "Failed")],
                60,
                240
            )),
            ["entra-login"]
        );
        assert_eq!(
            failed(&evaluate(Some(&vm("Linux")), &ok, 241, 240)),
            ["duration"]
        );
        assert_eq!(
            failed(&evaluate(Some(&vm("Linux")), &ok, 0, 240)),
            ["duration"]
        );
        assert_eq!(failed(&evaluate(None, &ok, 60, 240)), ["exists"]);
    }

    #[test]
    fn only_its_own_assignments_expire() {
        let ours = assignment(USER_LOGIN_ROLE, "User", Some(&description(1_790_787_600)));

        assert_eq!(ours.expires_at(), Some(1_790_787_600));
        assert_eq!(
            assignment(
                ADMIN_LOGIN_ROLE,
                "User",
                Some("plural.sh ssh-access expires=5")
            )
            .expires_at(),
            Some(5)
        );
        // Someone's permanent assignment, another role or a group is never touched.
        for other in [
            assignment(USER_LOGIN_ROLE, "User", None),
            assignment(USER_LOGIN_ROLE, "User", Some("break-glass access")),
            assignment(
                "8e3af657-a8ff-443c-a75c-2fe8c4bcb635",
                "User",
                Some(&description(5)),
            ),
            assignment(USER_LOGIN_ROLE, "Group", Some(&description(5))),
            assignment(
                USER_LOGIN_ROLE,
                "User",
                Some("plural.sh ssh-access expires=soon"),
            ),
        ] {
            assert_eq!(
                other.expires_at(),
                None,
                "{:?}",
                other.properties.description
            );
        }
    }

    #[test]
    fn describes_the_expiry_readably() {
        assert_eq!(
            description(1_790_787_600),
            "plural.sh ssh-access expires=1790787600 (2026-09-30T17:00:00Z)"
        );
    }

    #[test]
    fn connects_through_bastion_when_configured() {
        let bastion = bastion(Some(&format!(
            "/subscriptions/{SUB}/resourceGroups/net/providers/Microsoft.Network/bastionHosts/hub"
        )))
        .unwrap();

        assert_eq!(
            connect_command(&vm_id(), bastion.as_ref()),
            format!(
                "az network bastion ssh --subscription {SUB} --resource-group net --name hub --target-resource-id {} --auth-type AAD",
                vm_id().id()
            )
        );
        assert_eq!(
            connect_command(&vm_id(), None),
            format!("az ssh vm --ids {}", vm_id().id())
        );
    }

    #[test]
    fn refuses_a_bastion_id_that_isnt_one() {
        assert_eq!(bastion(None), Ok(None));
        assert_eq!(bastion(Some(" ")), Ok(None));
        for bad in [
            "not-an-id".to_owned(),
            format!(
                "/subscriptions/{SUB}/resourceGroups/net/providers/Microsoft.Network/virtualNetworks/hub"
            ),
        ] {
            let err = bastion(Some(&bad)).unwrap_err();
            assert!(err.contains("BASTION_ID"), "{err}");
        }
    }

    #[test]
    fn drops_trailing_slashes_of_scopes() {
        let rg = format!("/subscriptions/{SUB}/resourceGroups/vms");
        assert_eq!(
            scopes(Some(
                &serde_json::to_string(&[format!("{rg}/"), rg.clone()]).unwrap()
            )),
            Ok(vec![rg.clone(), rg])
        );
        assert_eq!(scopes(None), Ok(vec![]));
        assert!(scopes(Some(r#"["/"]"#)).is_err());
        assert!(scopes(Some("not json")).is_err());
    }

    #[test]
    fn maps_roles_and_generates_guids() {
        assert_eq!(
            Role::of(&format!(
                "/x/roleDefinitions/{}",
                ADMIN_LOGIN_ROLE.to_uppercase()
            )),
            Some(Role::Admin)
        );
        assert_eq!(Role::of("/x/roleDefinitions/other"), None);
        let (a, b) = (new_guid(), new_guid());
        assert!(arm::is_guid(&a) && arm::is_guid(&b) && a != b, "{a} {b}");
        assert_eq!(max_duration(None), DEFAULT_MAX_DURATION_MINUTES);
        assert_eq!(max_duration(Some("30")), 30);
        assert_eq!(max_duration(Some("-1")), DEFAULT_MAX_DURATION_MINUTES);
    }
}

#[cfg(test)]
mod handler_tests {
    use functions_azure::mock::{Method, MockArm};
    use serde_json::Value;

    use super::*;

    const SUB: &str = "00000000-0000-0000-0000-000000000000";
    const USER: &str = "11111111-1111-1111-1111-111111111111";
    const OTHER: &str = "22222222-2222-2222-2222-222222222222";

    fn rg(name: &str) -> String {
        format!("/subscriptions/{SUB}/resourceGroups/{name}")
    }

    fn vm() -> String {
        format!(
            "{}/providers/Microsoft.Compute/virtualMachines/vm-1",
            rg("rg")
        )
    }

    fn assignments_path(scope: &str) -> String {
        format!("{scope}/providers/Microsoft.Authorization/roleAssignments")
    }

    fn assignment(
        name: &str,
        scope: &str,
        principal: &str,
        role: &str,
        description: Option<String>,
    ) -> Value {
        json!({
            "id": format!("{}/{name}", assignments_path(scope)), "name": name,
            "properties": {
                "roleDefinitionId": format!("/subscriptions/{SUB}/providers/Microsoft.Authorization/roleDefinitions/{role}"),
                "principalId": principal, "principalType": "User", "scope": scope,
                "description": description,
            }
        })
    }

    /// A Linux VM with Entra ID login and these role assignments at or above it.
    fn script_vm(mock: &MockArm, assignments: Vec<Value>, with_login: bool) {
        let extensions = if with_login {
            json!([{"properties": {"type": "AADSSHLoginForLinux", "provisioningState": "Succeeded"}}])
        } else {
            json!([])
        };
        mock.on(Method::GET, &vm(), 200, json!({"name": "vm-1", "properties": {"storageProfile": {"osDisk": {"osType": "Linux"}}}}))
            .on(Method::GET, &format!("{}/extensions", vm()), 200, json!({"value": extensions}))
            .on(Method::GET, &assignments_path(&vm()), 200, json!({"value": assignments}))
            .on_any(Method::PUT, 201, json!({}));
    }

    async fn call(mock: &MockArm, input: Value) -> Value {
        let req = serde_json::from_value(input).unwrap();
        serde_json::to_value(handle(&mock.connector(), req).await.unwrap()).unwrap()
    }

    fn methods(mock: &MockArm) -> Vec<(Method, String)> {
        mock.writes()
            .into_iter()
            .map(|w| (w.method, w.path.rsplit('/').next().unwrap().to_owned()))
            .collect()
    }

    #[tokio::test]
    async fn plan_reads_only() {
        let mock = MockArm::start().await;
        script_vm(&mock, vec![], true);

        let resp = call(
            &mock,
            json!({"action": "plan", "vmId": vm(), "principalId": USER}),
        )
        .await;

        assert_eq!(resp["outcome"], "planned", "{resp}");
        assert_eq!(
            resp["result"]["command"],
            format!("az ssh vm --ids {}", vm())
        );
        assert!(mock.writes().is_empty());
    }

    #[tokio::test]
    async fn grants_the_login_role_and_removes_expired_access() {
        let mock = MockArm::start().await;
        let expired = assignment(
            "old",
            &vm(),
            OTHER,
            USER_LOGIN_ROLE,
            Some(description(now() - 60)),
        );
        script_vm(&mock, vec![expired], true);
        mock.on(
            Method::DELETE,
            &format!("{}/old", assignments_path(&vm())),
            200,
            json!({}),
        );

        let resp = call(&mock, json!({"action": "execute", "vmId": vm(), "principalId": USER, "role": "admin", "durationMinutes": 30})).await;

        assert_eq!(resp["outcome"], "done", "{resp}");
        assert_eq!(resp["result"]["removed"], 1);
        let writes = mock.writes();
        assert_eq!(writes[0].method, Method::DELETE);
        let put = &writes[1];
        assert_eq!(put.method, Method::PUT);
        assert!(
            put.path
                .starts_with(&format!("{}/", assignments_path(&vm())))
        );
        assert!(arm::is_guid(put.path.rsplit('/').next().unwrap()));
        let props = &put.body["properties"];
        assert_eq!(
            props["roleDefinitionId"],
            format!(
                "/subscriptions/{SUB}/providers/Microsoft.Authorization/roleDefinitions/{ADMIN_LOGIN_ROLE}"
            )
        );
        assert_eq!(
            (
                props["principalId"].as_str(),
                props["principalType"].as_str()
            ),
            (Some(USER), Some("User"))
        );
        let expires = Assignment {
            properties: AssignmentProperties {
                role_definition_id: ADMIN_LOGIN_ROLE.into(),
                principal_type: Some("User".into()),
                description: props["description"].as_str().map(str::to_owned),
                ..Default::default()
            },
            ..Default::default()
        }
        .expires_at()
        .unwrap();
        assert!(
            (expires - now() - 30 * 60).abs() <= 5,
            "expires in 30 minutes"
        );
    }

    #[tokio::test]
    async fn granting_again_never_shortens_access() {
        let mock = MockArm::start().await;
        let later = now() + 200 * 60;
        script_vm(
            &mock,
            vec![assignment(
                "mine",
                &vm(),
                USER,
                USER_LOGIN_ROLE,
                Some(description(later)),
            )],
            true,
        );

        call(
            &mock,
            json!({"action": "execute", "vmId": vm(), "principalId": USER, "durationMinutes": 10}),
        )
        .await;

        let writes = mock.writes();
        assert_eq!(methods(&mock), [(Method::PUT, "mine".to_owned())]);
        assert_eq!(
            writes[0].body["properties"]["description"],
            description(later)
        );
    }

    #[tokio::test]
    async fn extends_the_grant_a_concurrent_call_created_first() {
        let mock = MockArm::start().await;
        let theirs = now() + 60 * 60;
        script_vm(&mock, vec![], true);
        // The other call's assignment appears after this one read the VM's assignments.
        mock.on(
            Method::GET,
            &assignments_path(&vm()),
            200,
            json!({"value": [assignment("theirs", &vm(), USER, USER_LOGIN_ROLE, Some(description(theirs)))]}),
        )
        .on_any(
            Method::PUT,
            409,
            json!({"error": {"code": "RoleAssignmentExists", "message": "The role assignment already exists."}}),
        )
        .on(Method::PUT, &format!("{}/theirs", assignments_path(&vm())), 200, json!({}));

        let resp = call(
            &mock,
            json!({"action": "execute", "vmId": vm(), "principalId": USER, "durationMinutes": 10}),
        )
        .await;

        assert_eq!(resp["outcome"], "done", "{resp}");
        assert_eq!(resp["result"]["granted"], true);
        assert_eq!(resp["result"]["expiresAt"], format_time(theirs));
        let writes = mock.writes();
        assert_eq!(writes.len(), 2);
        assert!(arm::is_guid(writes[0].path.rsplit('/').next().unwrap()));
        assert!(writes[1].path.ends_with("/theirs"));
        // The longer of the two grants wins.
        assert_eq!(
            writes[1].body["properties"]["description"],
            description(theirs)
        );
    }

    #[tokio::test]
    async fn revoke_removes_only_its_own_grants_and_reports_other_access() {
        let mock = MockArm::start().await;
        script_vm(
            &mock,
            vec![
                assignment(
                    "mine",
                    &vm(),
                    USER,
                    USER_LOGIN_ROLE,
                    Some(description(now() + 600)),
                ),
                assignment(
                    "theirs",
                    &vm(),
                    OTHER,
                    USER_LOGIN_ROLE,
                    Some(description(now() + 600)),
                ),
                assignment(
                    "permanent",
                    &rg("rg"),
                    USER,
                    ADMIN_LOGIN_ROLE,
                    Some("break-glass".into()),
                ),
            ],
            true,
        );
        mock.on(
            Method::DELETE,
            &format!("{}/mine", assignments_path(&vm())),
            200,
            json!({}),
        );

        let resp = call(
            &mock,
            json!({"action": "execute", "vmId": vm(), "principalId": USER, "revoke": true}),
        )
        .await;

        assert_eq!(resp["outcome"], "done");
        assert_eq!(methods(&mock), [(Method::DELETE, "mine".to_owned())]);
        assert_eq!(resp["result"]["otherAccess"], json!([rg("rg")]));
    }

    #[tokio::test]
    async fn refuses_vms_without_entra_login() {
        let mock = MockArm::start().await;
        script_vm(&mock, vec![], false);

        let resp = call(
            &mock,
            json!({"action": "execute", "vmId": vm(), "principalId": USER}),
        )
        .await;

        assert_eq!(resp["outcome"], "refused");
        assert!(mock.writes().is_empty());
    }

    #[tokio::test]
    async fn sweep_continues_past_failing_scopes() {
        let mock = MockArm::start().await;
        let (broken, ok) = (rg("broken"), rg("ok"));
        let vm_scope = format!("{ok}/providers/Microsoft.Compute/virtualMachines/vm-2");
        mock.on(Method::GET, &assignments_path(&broken), 403, json!({"error": {"code": "AuthorizationFailed", "message": "boom"}}))
            .on(
                Method::GET,
                &assignments_path(&ok),
                200,
                json!({"value": [
                    assignment("expired", &vm_scope, USER, USER_LOGIN_ROLE, Some(description(now() - 60))),
                    assignment("active", &vm_scope, USER, USER_LOGIN_ROLE, Some(description(now() + 600))),
                    assignment("manual", &vm_scope, OTHER, USER_LOGIN_ROLE, None),
                    // Assigned at the resource group itself, not below it: left to whoever made it.
                    assignment("at-group", &ok, USER, USER_LOGIN_ROLE, Some(description(now() - 60))),
                ]}),
            )
            .on(Method::DELETE, &format!("{}/expired", assignments_path(&vm_scope)), 200, json!({}));

        let err = expire(&mock.connector(), &[broken, ok]).await.unwrap_err();

        assert!(err.to_string().contains("boom"), "{err}");
        assert_eq!(methods(&mock), [(Method::DELETE, "expired".to_owned())]);
    }
}
