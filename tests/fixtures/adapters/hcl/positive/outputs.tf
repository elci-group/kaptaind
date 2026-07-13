output "endpoint" {
  value = aws_instance.web.public_dns
}

output "bucket_arn" {
  value = aws_s3_bucket.assets.arn
}
