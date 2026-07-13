variable "region" {
  type    = string
  default = "us-east-1"
}

output "endpoint" {
  value = aws_instance.web.public_dns
}

resource "aws_instance" "web" {
  ami           = data.aws_ami.ubuntu.id
  instance_type = "t3.micro"
}

data "aws_ami" "ubuntu" {
  most_recent = true
}

module "network" {
  source = "./modules/network"
}
