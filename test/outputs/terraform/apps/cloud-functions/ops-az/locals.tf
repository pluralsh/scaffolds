locals {
  # Without allow_skip_snapshot, the `snapshot` input is removed from the tool schema and the
  # function rejects `snapshot: false` as well.
  volume_delete_schema = jsondecode(file("${path.module}/schemas/volume-delete.json"))
  volume_delete_properties = {
    for key, prop in local.volume_delete_schema.properties : key => prop if key != "snapshot" || var.allow_skip_snapshot
  }

  # Every function that can be deployed. `function` is the HTTP function inside the app (the
  # folder holding its function.json in the package), `actions` is the minimal set of ARM
  # actions the function needs, which its managed identity gets on the resource groups in
  # var.scopes, and `condition` an optional condition on those role assignments.
  # `network_actions` are the join actions it also gets on the resource groups in
  # var.network_scopes, and `subscription_actions` the reads it gets on the subscription.
  # Every function is registered as a workbench tool, and every call of the tools of
  # `destructive` functions, which change or delete resources, requires human approval.
  catalog = {
    volume-delete = {
      binary      = "volume-delete-azure"
      function    = "volume-delete"
      description = "Deletes an unattached managed disk that Kubernetes created for a PersistentVolume. Before calling it, confirm in the cluster that the PersistentVolume no longer exists and pass its name as pvName. Use action plan first; execute takes an incremental snapshot and keeps the disk, and a later execute deletes it once the snapshot has completed."
      destructive = true
      environment = { ALLOW_SKIP_SNAPSHOT = tostring(var.allow_skip_snapshot) }
      actions = [
        "Microsoft.Compute/disks/read",
        "Microsoft.Compute/disks/delete",
        # Creating a snapshot copies the source disk through a read access grant.
        "Microsoft.Compute/disks/beginGetAccess/action",
        "Microsoft.Compute/snapshots/read",
        "Microsoft.Compute/snapshots/write",
      ]
      condition            = null
      network_actions      = []
      subscription_actions = []
      schema               = jsonencode(merge(local.volume_delete_schema, { properties = local.volume_delete_properties }))
    }
    node-pool-resize = {
      binary      = "node-pool-resize-azure"
      function    = "node-pool-resize"
      description = "Sets the node count of a manually scaled AKS node pool; AKS drains the nodes it removes. Pools scaled by the cluster autoscaler are refused. Use action plan first to see the current count and the checks, then execute."
      destructive = true
      environment = { MAX_NODE_COUNT = tostring(var.node_pool_max_count) }
      # The update resends the whole pool, and ARM checks the caller may join the subnets and
      # public IP prefix it references.
      actions = concat([
        "Microsoft.ContainerService/managedClusters/agentPools/read",
        "Microsoft.ContainerService/managedClusters/agentPools/write",
      ], local.node_pool_join_actions)
      condition = null
      # A BYO VNet outside the cluster's resource group needs its resource group in
      # network_scopes.
      network_actions      = local.node_pool_join_actions
      subscription_actions = []
      schema               = jsonencode(jsondecode(file("${path.module}/schemas/node-pool-resize.json")))
    }
    vm-delete = {
      binary      = "vm-delete-azure"
      function    = "vm-delete"
      description = "Deletes a standalone VM with its OS disk and network interfaces; data disks are detached and kept. VMs of scale sets and AKS nodes are refused. Use action plan first to see what is deleted and kept, then execute; execute again if it reports that the VM is still updating."
      destructive = true
      environment = {}
      actions = [
        "Microsoft.Compute/virtualMachines/read",
        "Microsoft.Compute/virtualMachines/write",
        "Microsoft.Compute/virtualMachines/delete",
        # The VM update references its disks and network interfaces, and deleting the VM
        # deletes the OS disk and network interfaces with it.
        "Microsoft.Compute/disks/read",
        "Microsoft.Compute/disks/write",
        "Microsoft.Compute/disks/delete",
        "Microsoft.Network/networkInterfaces/read",
        "Microsoft.Network/networkInterfaces/join/action",
        "Microsoft.Network/networkInterfaces/delete",
      ]
      condition            = null
      network_actions      = []
      subscription_actions = []
      schema               = jsonencode(jsondecode(file("${path.module}/schemas/vm-delete.json")))
    }
    lb-frontend-delete = {
      binary      = "lb-frontend-delete-azure"
      function    = "lb-frontend-delete"
      description = "Removes what a deleted Kubernetes LoadBalancer Service left on an AKS load balancer: its frontend, rules and probes, then its public IP. Before calling it, confirm in the cluster that no Service has the UID in the frontend name. Use action plan first; each execute makes one change, so execute again while the result reports remaining: true."
      destructive = true
      environment = {}
      actions = [
        "Microsoft.Network/loadBalancers/read",
        "Microsoft.Network/loadBalancers/write",
        "Microsoft.Network/loadBalancers/delete",
        "Microsoft.Network/publicIPAddresses/read",
        "Microsoft.Network/publicIPAddresses/delete",
        # Updating the load balancer references the public IPs and subnets of its other frontends.
        "Microsoft.Network/publicIPAddresses/join/action",
        "Microsoft.Network/virtualNetworks/subnets/join/action",
        # Backend health of the frontend's rules, checked before removing it.
        "Microsoft.Network/loadBalancers/loadBalancingRules/health/action",
      ]
      condition       = null
      network_actions = []
      # The backend health action is long-running, and ARM serves its result at subscription
      # scope.
      subscription_actions = [
        "Microsoft.Network/locations/operationResults/read",
        "Microsoft.Network/locations/operations/read",
      ]
      schema = jsonencode(jsondecode(file("${path.module}/schemas/lb-frontend-delete.json")))
    }
    db-restore = {
      binary      = "db-restore-azure"
      function    = "db-restore"
      description = "Restores a PostgreSQL or MySQL flexible server to a point in time as a new server in the same resource group; the source server is never changed. Use action plan first to see the earliest restore point, then execute, and call again with the same parameters to see the new server's state and hostname."
      destructive = true
      environment = {}
      actions = [
        "Microsoft.DBforPostgreSQL/flexibleServers/read",
        "Microsoft.DBforPostgreSQL/flexibleServers/write",
        "Microsoft.DBforMySQL/flexibleServers/read",
        "Microsoft.DBforMySQL/flexibleServers/write",
        # Servers in a virtual network are restored into the source's subnet and private DNS zone.
        "Microsoft.Network/virtualNetworks/subnets/join/action",
        "Microsoft.Network/privateDnsZones/join/action",
      ]
      condition            = null
      network_actions      = []
      subscription_actions = []
      schema               = jsonencode(jsondecode(file("${path.module}/schemas/db-restore.json")))
    }
    ssh-access = {
      binary      = "ssh-access-azure"
      function    = "ssh-access"
      description = "Grants an Entra ID user short-lived SSH login to a Linux VM with Entra ID login enabled, by assigning the Virtual Machine User (or Administrator) Login role on the VM until it expires, and returns the command to connect. revoke: true removes the access right away. Use action plan first to check the VM, then execute."
      destructive = true
      environment = merge(
        {
          MAX_DURATION_MINUTES = tostring(var.ssh_access_max_minutes)
          # Resource groups the timer removes expired access in.
          SCOPES = jsonencode(lookup(var.scopes, "ssh-access", []))
        },
        { for key, value in { BASTION_ID = var.ssh_bastion_id } : key => value if value != null },
      )
      actions = [
        "Microsoft.Compute/virtualMachines/read",
        "Microsoft.Compute/virtualMachines/extensions/read",
        "Microsoft.Authorization/roleAssignments/read",
        "Microsoft.Authorization/roleAssignments/write",
        "Microsoft.Authorization/roleAssignments/delete",
      ]
      condition            = local.ssh_access_condition
      network_actions      = []
      subscription_actions = []
      schema               = jsonencode(jsondecode(file("${path.module}/schemas/ssh-access.json")))
    }
  }

  # Limits the role assignments ssh-access may create and delete to the VM login roles and to
  # users, so it can't grant anything else.
  vm_login_roles = "fb879df8-f326-4884-b1cf-06f3ad86be52, 1c0163c0-47e6-4577-8991-ea5c82e286e4"
  ssh_access_condition = join(" AND ", [
    for action, source in { write = "Request", delete = "Resource" } :
    "((!(ActionMatches{'Microsoft.Authorization/roleAssignments/${action}'})) OR (@${source}[Microsoft.Authorization/roleAssignments:RoleDefinitionId] ForAnyOfAnyValues:GuidEquals {${local.vm_login_roles}} AND @${source}[Microsoft.Authorization/roleAssignments:PrincipalType] ForAnyOfAnyValues:StringEqualsIgnoreCase {'User'}))"
  ])

  identity_context    = jsondecode(data.plural_service_context.identity.configuration)
  cluster_context     = jsondecode(data.plural_service_context.cluster.configuration)
  resource_group_name = coalesce(var.resource_group_name, local.cluster_context.resource_group_name)

  node_pool_join_actions = [
    "Microsoft.Network/virtualNetworks/subnets/join/action",
    "Microsoft.Network/publicIPPrefixes/join/action",
  ]

  functions = {
    for key, fn in local.catalog : key => merge(fn, {
      scopes         = lookup(var.scopes, key, [])
      network_scopes = lookup(var.network_scopes, key, [])
    }) if contains(var.functions, key)
  }
  unknown         = setsubtract(concat(var.functions, keys(var.scopes), keys(var.network_scopes)), keys(local.catalog))
  subscription_id = "/subscriptions/${local.identity_context["subscription_id"]}"
  # The version is part of the path, so a new release changes zip_deploy_file and redeploys.
  artifacts = { for key, fn in local.functions : key => "${var.artifact_dir}/${var.artifact_version}/${fn.binary}.zip" }

  # Function app and storage account names are globally unique, so they get a suffix derived
  # from the subscription, resource group and installation name. App names are limited to 32
  # characters and storage account names to 24 lowercase letters and digits.
  hash                 = sha1("${local.identity_context["subscription_id"]}/${local.resource_group_name}/${var.name}")
  app_names            = { for key, _ in local.functions : key => "${trim(substr("${var.name}-${key}", 0, 25), "-")}-${substr(local.hash, 0, 6)}" }
  storage_account_name = "plrlfn${substr(local.hash, 0, 18)}"
}
