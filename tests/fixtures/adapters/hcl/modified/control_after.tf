output "endpoint" {
  value       = aws_instance.web.public_dns
  description = "Public DNS of the web instance"
}
