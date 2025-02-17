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
  {% if context.repository != nil and context.repository != '' %}
  default = "{{ context.repository }}"
  {% else %}
  default = "pluralos-{{ context.name }}"
  {% endif %}
}

variable "namespace" {
  type = string
  default = "osbuilder"
}

variable "service_account_name" {
  type = string
  default = "pluralos-{{ context.name }}-osartifact"
}