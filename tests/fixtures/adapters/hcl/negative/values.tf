terraform {
  required_version = ">= 1.5"
  backend "s3" {
    bucket = "tf-state"
  }
}

locals {
  name_prefix = "prod"
  tags = {
    env = "prod"
  }
}
