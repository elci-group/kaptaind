# variable "commented_hash" {
#   type = string
# }

// variable "commented_slash" {
//   type = string
// }

/*
output "commented_block" {
  value = "nope"
}
*/

locals {
  script = <<-EOF
    #!/bin/bash
    resource "aws_instance" "heredoc_fake" {
      ami = "ami-fake"
    }
  EOF
}

resource "aws_s3_bucket" "assets" {
  bucket = "assets"
}
