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

# A healthy S3 listener does not imply that the application bucket exists.
# Bootstrap owns bucket creation; only a HeadBucket 404 means it is missing.
echo "Checking S3 bucket: $LOOP_S3_BUCKET"
if aws s3api head-bucket --bucket "$LOOP_S3_BUCKET" 2>/tmp/loop-s3-head-error; then
  echo "S3 bucket already exists: $LOOP_S3_BUCKET"
elif grep -Fq 'An error occurred (404) when calling the HeadBucket operation' /tmp/loop-s3-head-error; then
  echo "Creating S3 bucket: $LOOP_S3_BUCKET (region=$AWS_DEFAULT_REGION)"
  if [ "$AWS_DEFAULT_REGION" = "us-east-1" ]; then
    aws s3api create-bucket --bucket "$LOOP_S3_BUCKET"
  else
    aws s3api create-bucket --bucket "$LOOP_S3_BUCKET" \
      --create-bucket-configuration "LocationConstraint=$AWS_DEFAULT_REGION"
  fi
else
  cat /tmp/loop-s3-head-error >&2
  exit 1
fi

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
