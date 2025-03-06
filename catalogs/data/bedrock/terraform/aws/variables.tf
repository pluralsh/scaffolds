variable "cluster" {
  type = string
  default = "{{ context.cluster }}"
}

variable "region" {
  type    = string
  default = "{{ context.region }}"
}
