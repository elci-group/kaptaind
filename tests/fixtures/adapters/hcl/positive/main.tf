provider "aws" {
  region = var.region
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
