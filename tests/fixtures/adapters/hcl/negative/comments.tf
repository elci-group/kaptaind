# variable "fake_hash" {
#   type = string
# }

// output "fake_slash" {
//   value = "nope"
// }

/*
resource "aws_instance" "fake_block" {
  ami = "ami-fake"
}
*/

locals {
  user_data = <<-EOF
    #!/bin/bash
    resource "aws_s3_bucket" "fake_heredoc" {
      bucket = "fake"
    }
  EOF
}
