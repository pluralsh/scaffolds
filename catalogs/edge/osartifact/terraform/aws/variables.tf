variable "cluster_name" {
  type = string
  default = "{{ context.cluster }}"
}

variable "osartifact_name" {
  type = string
  default = "{{ context.name }}"
}

variable "repository_name" {
  type = string
  default = "{{ context.repository }}"
}

variable "namespace" {
  type = string
  default = "{{ context.namespace }}"
}

variable "service_account_name" {
  type = string
  default = "{{ context.serviceAccountName }}"
}