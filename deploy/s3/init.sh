#!/bin/sh
set -eu

: "${LOOP_S3_BUCKET:?missing LOOP_S3_BUCKET}"
: "${AWS_ENDPOINT_URL:?missing AWS_ENDPOINT_URL}"
: "${AWS_ACCESS_KEY_ID:?missing AWS_ACCESS_KEY_ID}"
: "${AWS_SECRET_ACCESS_KEY:?missing AWS_SECRET_ACCESS_KEY}"
: "${AWS_DEFAULT_REGION:?missing AWS_DEFAULT_REGION}"
export AWS_PAGER=""
export AWS_EC2_METADATA_DISABLED=true

trap 'status=$?; if [ "$status" -ne 0 ]; then echo "S3 bucket initialization failed (exit=$status)" >&2; fi' EXIT

# SeaweedFS mini creates the bucket before opening its S3 listener.
# Fail on configuration/authentication errors instead of treating them as a missing bucket.
echo "Checking S3 bucket: $LOOP_S3_BUCKET"
aws s3api head-bucket --bucket "$LOOP_S3_BUCKET"

echo "Configuring anonymous GetObject access for bucket: $LOOP_S3_BUCKET"
cat > /tmp/loop-s3-public-read.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [{
    "Sid": "PublicReadMedia",
    "Effect": "Allow",
    "Principal": "*",
    "Action": "s3:GetObject",
    "Resource": "arn:aws:s3:::$LOOP_S3_BUCKET/*"
  }]
}
EOF
aws s3api put-bucket-policy --bucket "$LOOP_S3_BUCKET" \
  --policy file:///tmp/loop-s3-public-read.json

echo "Configuring browser upload/download CORS for bucket: $LOOP_S3_BUCKET"
aws s3api put-bucket-cors --bucket "$LOOP_S3_BUCKET" \
  --cors-configuration file:///etc/loop/s3/cors.json
echo "S3 bucket initialized: $LOOP_S3_BUCKET (public read; authenticated writes)"
