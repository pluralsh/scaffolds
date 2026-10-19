#!/usr/bin/env bash
# Creates, shows and removes the Azure resources the end-to-end checklist acts on. Every
# resource lives in resource groups named after E2E_PREFIX and tagged plural-e2e=<prefix>, so
# cleanup only ever deletes those. Usually run as `just fixtures-azure <action>` from functions/.
#
#   up       create everything (idempotent, ~20 minutes, mostly AKS and PostgreSQL)
#   status   show what exists
#   down     delete the resource groups
#
# Optional settings:
#   E2E_MYSQL=1         also create a MySQL flexible server for db-restore
#   E2E_SSH_PUBLIC=1    give vm-keep a public IP reachable on port 22 from this machine's
#                       public IP only, to test a real Entra ID SSH login
#   E2E_PRINCIPAL=<id>  Entra ID user object ID ssh-access grants to (default: the signed-in user)
#   E2E_DB_LOCATION     location of the database servers (default eastus2: subscriptions are often
#                       restricted from creating flexible servers in eastus)
#
# The IDs the checklist uses are recorded in .work/fixtures.env (source it).

source "$(dirname "$0")/lib.sh"

VM_SIZE="${E2E_VM_SIZE:-Standard_B1s}"
NODE_SIZE="${E2E_NODE_SIZE:-Standard_B2s}"
KUBECONFIG_FILE="$WORK/kubeconfig"
DB_LOCATION="${E2E_DB_LOCATION:-eastus2}"

exists() { "$@" >/dev/null 2>&1; }

group() {
  local name="$1"
  if exists az group show -n "$name"; then
    local tag
    tag="$(az group show -n "$name" --query "tags.\"$TAG_KEY\"" -o tsv)"
    [[ "$tag" == "$P" ]] || die "resource group $name exists without tag $TAG_KEY=$P; pick another E2E_PREFIX"
    return
  fi
  log "resource group $name"
  az group create -n "$name" -l "$LOCATION" --tags "$TAG_KEY=$P" -o none
}

aks() {
  if ! exists az aks show -g "$RG_AKS" -n "$AKS"; then
    exists az group show -n "$RG_NODE" && die "$RG_NODE exists but $AKS doesn't; AKS must create its node resource group"
    log "AKS cluster $AKS (node resource group $RG_NODE)"
    az aks create -g "$RG_AKS" -n "$AKS" -l "$LOCATION" \
      --node-resource-group "$RG_NODE" --tier free --no-ssh-key \
      --nodepool-name system --node-count 1 --node-vm-size "$NODE_SIZE" \
      --tags "$TAG_KEY=$P" -o none
  fi
  if ! exists az aks nodepool show -g "$RG_AKS" --cluster-name "$AKS" -n manual; then
    log "node pool manual (User, 1 node, no autoscaler)"
    az aks nodepool add -g "$RG_AKS" --cluster-name "$AKS" -n manual --mode User \
      --node-count 1 --node-vm-size "$NODE_SIZE" -o none
  fi
  if ! exists az aks nodepool show -g "$RG_AKS" --cluster-name "$AKS" -n auto; then
    log "node pool auto (User, autoscaled 0-1)"
    az aks nodepool add -g "$RG_AKS" --cluster-name "$AKS" -n auto --mode User \
      --enable-cluster-autoscaler --min-count 0 --max-count 1 --node-count 0 \
      --node-vm-size "$NODE_SIZE" -o none
  fi
  az aks get-credentials -g "$RG_AKS" -n "$AKS" -f "$KUBECONFIG_FILE" --overwrite-existing -o none
  record CLUSTER_ID "$(az aks show -g "$RG_AKS" -n "$AKS" --query id -o tsv)"
}

kube() { KUBECONFIG="$KUBECONFIG_FILE" kubectl "$@"; }

# A disk the Azure Disk CSI driver created for a PersistentVolume that was then deleted with
# reclaim policy Retain: the real thing volume-delete is for, with the tags AKS sets.
orphaned_disk() {
  command -v kubectl >/dev/null || die "kubectl is required"
  if [[ -n "${ORPHAN_DISK:-}" ]] && exists az disk show --ids "$ORPHAN_DISK"; then
    return
  fi
  log "orphaned PersistentVolume disk"
  kube apply -f - >/dev/null <<'YAML'
apiVersion: v1
kind: Namespace
metadata:
  name: e2e
---
apiVersion: storage.k8s.io/v1
kind: StorageClass
metadata:
  name: e2e-retain
provisioner: disk.csi.azure.com
parameters:
  skuName: StandardSSD_LRS
reclaimPolicy: Retain
volumeBindingMode: Immediate
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: data
  namespace: e2e
spec:
  storageClassName: e2e-retain
  accessModes: [ReadWriteOnce]
  resources:
    requests:
      storage: 1Gi
YAML
  kube -n e2e wait --for=jsonpath='{.status.phase}'=Bound pvc/data --timeout=5m >/dev/null
  local pv disk
  pv="$(kube -n e2e get pvc data -o jsonpath='{.spec.volumeName}')"
  disk="$(kube get pv "$pv" -o jsonpath='{.spec.csi.volumeHandle}')"
  kube -n e2e delete pvc data --wait=true >/dev/null
  kube delete pv "$pv" --wait=true >/dev/null
  record ORPHAN_DISK "$disk"
  record ORPHAN_PV "$pv"
}

# A LoadBalancer Service with ready endpoints: its frontend on the AKS-managed kubernetes LB
# has healthy backends, so lb-frontend-delete must refuse it.
live_service() {
  log "LoadBalancer Service e2e/web"
  kube -n e2e create deployment web --image=nginx:1.27 --dry-run=client -o yaml | kube apply -f - >/dev/null
  kube -n e2e expose deployment web --type=LoadBalancer --port 80 --dry-run=client -o yaml | kube apply -f - >/dev/null
  kube -n e2e rollout status deployment/web --timeout=5m >/dev/null
  local pip=""
  for _ in $(seq 60); do
    pip="$(az network public-ip list -g "$RG_NODE" --query "[?tags.\"k8s-azure-service\"=='e2e/web'].name | [0]" -o tsv)"
    [[ -n "$pip" ]] && break
    sleep 10
  done
  [[ -n "$pip" ]] || die "the public IP of Service e2e/web didn't appear"
  record LIVE_LB "$(az network lb show -g "$RG_NODE" -n kubernetes --query id -o tsv)"
  record LIVE_FRONTEND "${pip#kubernetes-}"
}

# Load balancers shaped like the AKS one, with frontends of Services that no longer exist:
#   e2e-lb       FE_ORPHAN  (owned by e2e/orphan, the one to delete)
#                FE_OTHER   (its public IP belongs to e2e/other)
#                FE_OUTBOUND (also used by an outbound rule)
#   e2e-lb-last  FE_LAST    (the only frontend, empty backend pool: the whole LB goes)
synthetic_lbs() {
  FE_ORPHAN="a$(hex31 orphan)"
  FE_OTHER="a$(hex31 other)"
  FE_OUTBOUND="a$(hex31 outbound)"
  FE_LAST="a$(hex31 last)"
  record FE_ORPHAN "$FE_ORPHAN"
  record FE_OTHER "$FE_OTHER"
  record FE_OUTBOUND "$FE_OUTBOUND"
  record FE_LAST "$FE_LAST"

  # The checks remove FE_ORPHAN, and a failed run can leave the LB half built (the outbound
  # rule is added last); start over then.
  if exists az network lb show -g "$RG_NODE" -n e2e-lb &&
    { ! exists az network lb frontend-ip show -g "$RG_NODE" --lb-name e2e-lb -n "$FE_ORPHAN" ||
      ! exists az network lb outbound-rule show -g "$RG_NODE" --lb-name e2e-lb -n "$FE_OUTBOUND-outbound"; }; then
    log "recreating load balancer e2e-lb"
    az network lb delete -g "$RG_NODE" -n e2e-lb -o none
  fi
  if ! exists az network lb show -g "$RG_NODE" -n e2e-lb; then
    log "load balancer e2e-lb"
    pip "$FE_ORPHAN" e2e/orphan
    pip "$FE_OTHER" e2e/other
    pip "$FE_OUTBOUND" e2e/outbound
    az network lb create -g "$RG_NODE" -n e2e-lb --sku Standard \
      --frontend-ip-name "$FE_ORPHAN" --public-ip-address "kubernetes-$FE_ORPHAN" \
      --backend-pool-name kubernetes -o none
    for fe in "$FE_OTHER" "$FE_OUTBOUND"; do
      az network lb frontend-ip create -g "$RG_NODE" --lb-name e2e-lb -n "$fe" \
        --public-ip-address "kubernetes-$fe" -o none
    done
    for fe in "$FE_ORPHAN" "$FE_OTHER" "$FE_OUTBOUND"; do
      rule e2e-lb "$fe"
    done
    az network lb outbound-rule create -g "$RG_NODE" --lb-name e2e-lb -n "$FE_OUTBOUND-outbound" \
      --frontend-ip-configs "$FE_OUTBOUND" --protocol All --address-pool kubernetes -o none
  fi
  if exists az network lb show -g "$RG_NODE" -n e2e-lb-last &&
    ! exists az network lb rule show -g "$RG_NODE" --lb-name e2e-lb-last -n "$FE_LAST-TCP-80"; then
    log "recreating load balancer e2e-lb-last"
    az network lb delete -g "$RG_NODE" -n e2e-lb-last -o none
  fi
  if ! exists az network lb show -g "$RG_NODE" -n e2e-lb-last; then
    log "load balancer e2e-lb-last"
    pip "$FE_LAST" e2e/last
    az network lb create -g "$RG_NODE" -n e2e-lb-last --sku Standard \
      --frontend-ip-name "$FE_LAST" --public-ip-address "kubernetes-$FE_LAST" \
      --backend-pool-name kubernetes -o none
    rule e2e-lb-last "$FE_LAST"
  fi
  record SYN_LB "$(az network lb show -g "$RG_NODE" -n e2e-lb --query id -o tsv)"
  record SYN_LB_LAST "$(az network lb show -g "$RG_NODE" -n e2e-lb-last --query id -o tsv)"
}

pip() {
  exists az network public-ip show -g "$RG_NODE" -n "kubernetes-$1" && return
  az network public-ip create -g "$RG_NODE" -n "kubernetes-$1" --sku Standard \
    --allocation-method Static --tags "k8s-azure-service=$2" -o none
}

# Like AKS's Service rules: floating IP, so rules of several frontends can share the pool and port.
rule() {
  local lb="$1" fe="$2"
  az network lb probe create -g "$RG_NODE" --lb-name "$lb" -n "$fe-TCP-80" \
    --protocol Tcp --port 30080 -o none
  az network lb rule create -g "$RG_NODE" --lb-name "$lb" -n "$fe-TCP-80" \
    --protocol Tcp --frontend-port 80 --backend-port 80 --frontend-ip-name "$fe" \
    --backend-pool-name kubernetes --probe-name "$fe-TCP-80" --disable-outbound-snat true \
    --floating-ip true -o none
}

# Disks in the node resource group that volume-delete must refuse.
synthetic_disks() {
  if ! exists az disk show -g "$RG_NODE" -n e2e-notag; then
    log "disk e2e-notag (no Kubernetes tags)"
    az disk create -g "$RG_NODE" -n e2e-notag --size-gb 4 --sku StandardSSD_LRS -o none
  fi
  if ! exists az disk show -g "$RG_NODE" -n e2e-attached; then
    log "disk e2e-attached (Kubernetes tags, attached to vm-keep)"
    az disk create -g "$RG_NODE" -n e2e-attached --size-gb 4 --sku StandardSSD_LRS \
      --tags kubernetes.io-created-for-pv-name=pvc-e2e-attached \
      kubernetes.io-created-for-pvc-name=attached kubernetes.io-created-for-pvc-namespace=e2e -o none
  fi
  record NOTAG_DISK "$(az disk show -g "$RG_NODE" -n e2e-notag --query id -o tsv)"
  record ATTACHED_DISK "$(az disk show -g "$RG_NODE" -n e2e-attached --query id -o tsv)"
}

vms() {
  local key="$WORK/id_e2e"
  [[ -f "$key" ]] || ssh-keygen -q -t ed25519 -N '' -C "plural-e2e-$P" -f "$key"

  # vm-keep: Entra ID login, protected from vm-delete by an aks-managed- tag, holds e2e-attached.
  if ! exists az vm show -g "$RG_VMS" -n vm-keep; then
    log "VM vm-keep"
    local network=(--public-ip-address "" --nsg "")
    if [[ "${E2E_SSH_PUBLIC:-}" == 1 ]]; then
      network=(--public-ip-address vm-keep-ip --public-ip-sku Standard --nsg vm-keep-nsg --nsg-rule NONE)
    fi
    az vm create -g "$RG_VMS" -n vm-keep --image Ubuntu2204 --size "$VM_SIZE" \
      --admin-username azureuser --ssh-key-values "$key.pub" --assign-identity \
      "${network[@]}" --tags aks-managed-e2e=guard -o none
    az vm extension set -g "$RG_VMS" --vm-name vm-keep \
      --publisher Microsoft.Azure.ActiveDirectory --name AADSSHLoginForLinux -o none
    az vm disk attach -g "$RG_VMS" --vm-name vm-keep --name "$ATTACHED_DISK" -o none
    if [[ "${E2E_SSH_PUBLIC:-}" == 1 ]]; then
      local me
      me="$(curl -fsS https://api.ipify.org)"
      az network nsg rule create -g "$RG_VMS" --nsg-name vm-keep-nsg -n ssh-from-tester \
        --priority 100 --access Allow --protocol Tcp --destination-port-ranges 22 \
        --source-address-prefixes "$me/32" -o none
    fi
  fi
  record VM_KEEP "$(az vm show -g "$RG_VMS" -n vm-keep --query id -o tsv)"

  # vm-del: plain VM with a data disk, the one vm-delete deletes.
  if ! exists az vm show -g "$RG_VMS" -n vm-del; then
    log "VM vm-del"
    az vm create -g "$RG_VMS" -n vm-del --image Ubuntu2204 --size "$VM_SIZE" \
      --admin-username azureuser --ssh-key-values "$key.pub" \
      --public-ip-address "" --nsg "" --data-disk-sizes-gb 4 -o none
  fi
  local vm
  vm="$(az vm show -g "$RG_VMS" -n vm-del -o json)"
  record VM_DEL "$(jq -r .id <<<"$vm")"
  record VM_DEL_OS_DISK "$(jq -r .storageProfile.osDisk.managedDisk.id <<<"$vm")"
  record VM_DEL_DATA_DISK "$(jq -r '.storageProfile.dataDisks[0].managedDisk.id' <<<"$vm")"
  record VM_DEL_NIC "$(jq -r '.networkProfile.networkInterfaces[0].id' <<<"$vm")"
}

databases() {
  if [[ -z "${DB_PASSWORD:-}" ]]; then
    record DB_PASSWORD "E2e-$(openssl rand -hex 12)"
    load_fixtures
  fi
  if ! exists az postgres flexible-server show -g "$RG_DB" -n "$P-pg"; then
    log "PostgreSQL flexible server $P-pg (~10 minutes)"
    az postgres flexible-server create -g "$RG_DB" -n "$P-pg" -l "$DB_LOCATION" \
      --tier Burstable --sku-name Standard_B1ms --storage-size 32 --version 16 \
      --admin-user e2eadmin --admin-password "$DB_PASSWORD" --public-access None --yes -o none
  fi
  record PG_SERVER "$(az postgres flexible-server show -g "$RG_DB" -n "$P-pg" --query id -o tsv)"
  if [[ "${E2E_MYSQL:-}" == 1 ]]; then
    if ! exists az mysql flexible-server show -g "$RG_DB" -n "$P-my"; then
      log "MySQL flexible server $P-my (~10 minutes)"
      az mysql flexible-server create -g "$RG_DB" -n "$P-my" -l "$DB_LOCATION" \
        --tier Burstable --sku-name Standard_B1ms --storage-size 32 --version 8.0.21 \
        --admin-user e2eadmin --admin-password "$DB_PASSWORD" --public-access None --yes -o none
    fi
    record MY_SERVER "$(az mysql flexible-server show -g "$RG_DB" -n "$P-my" --query id -o tsv)"
  fi
}

up() {
  record SUB "$SUB"
  record PRINCIPAL "${E2E_PRINCIPAL:-$(az ad signed-in-user show --query id -o tsv)}"
  load_fixtures
  for rg in "$RG_AKS" "$RG_VMS" "$RG_DB"; do group "$rg"; done
  aks
  load_fixtures
  orphaned_disk
  live_service
  synthetic_disks
  load_fixtures
  synthetic_lbs
  vms
  databases
  log "fixtures ready, recorded in $FIXTURES_ENV"
}

status() {
  az group list --tag "$TAG_KEY=$P" --query '[].{group:name, location:location, state:properties.provisioningState}' -o table
  if exists az aks show -g "$RG_AKS" -n "$AKS"; then
    az aks nodepool list -g "$RG_AKS" --cluster-name "$AKS" \
      --query '[].{pool:name, mode:mode, count:count, autoscaling:enableAutoScaling, state:provisioningState}' -o table
  fi
  for rg in "$RG_NODE" "$RG_VMS" "$RG_DB"; do
    exists az group show -n "$rg" || continue
    echo
    echo "$rg:"
    az resource list -g "$rg" --query "[?type!='Microsoft.Compute/virtualMachineScaleSets'].{name:name, type:type}" -o table
  done
}

down() {
  # The node resource group goes with the cluster's resource group.
  for rg in "$RG_AKS" "$RG_VMS" "$RG_DB"; do
    exists az group show -n "$rg" || continue
    [[ "$(az group show -n "$rg" --query "tags.\"$TAG_KEY\"" -o tsv)" == "$P" ]] ||
      die "$rg isn't tagged $TAG_KEY=$P; not deleting it"
    log "deleting resource group $rg"
    az group delete -n "$rg" --yes --no-wait
  done
  rm -f "$FIXTURES_ENV" "$KUBECONFIG_FILE"
  log "deletion started; ./fixtures.sh status shows what is left"
}

[[ -f "$FIXTURES_ENV" ]] && load_fixtures
case "${1:-}" in
  up) up ;;
  status) status ;;
  down) down ;;
  *) echo "usage: $0 up|status|down" >&2; exit 2 ;;
esac
