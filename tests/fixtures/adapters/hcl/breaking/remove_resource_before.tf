resource "aws_instance" "web" {
  ami           = "ami-123"
  instance_type = "t3.micro"
}

resource "aws_s3_bucket" "assets" {
  bucket = "assets"
}
