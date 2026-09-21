# Repository tools

## `prove-aws.sh` — on-demand release measurements

Runs the ignored measurements of `crates/kaimeter-guest` (cycle counts,
proving time, image ID) on a disposable EC2 instance: the latest Ubuntu
24.04, the pinned rustup and RISC Zero toolchains installed by the remote
script, the repository cloned at a pushed ref, then the instance is
terminated.

Credentials and region follow the AWS CLI chain (`AWS_PROFILE`, SSO,
environment variables, instance roles). The only required input is an IAM
instance profile carrying `AmazonSSMManagedInstanceCore`, so the instance is
driven through SSM Session Manager without an SSH key or an inbound rule.

One-time setup:

```sh
cat > trust.json <<'EOF'
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Principal": { "Service": "ec2.amazonaws.com" },
    "Action": "sts:AssumeRole"
  }]
}
EOF
aws iam create-role --role-name kaimeter-prove \
  --assume-role-policy-document file://trust.json
aws iam attach-role-policy --role-name kaimeter-prove \
  --policy-arn arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore
aws iam create-instance-profile --instance-profile-name kaimeter-prove
aws iam add-role-to-instance-profile \
  --instance-profile-name kaimeter-prove --role-name kaimeter-prove
```

Usage:

```sh
AWS_PROFILE=keldrion tools/prove-aws.sh \
  --instance-profile kaimeter-prove --region ap-southeast-1
```

The default instance type is `c6i.2xlarge` (8 vCPU / 16 GB), the floor for
the local prover, with a 40 GB root volume (`--volume-size` overrides both).
A run costs instance-minutes only and the script terminates the instance when
it exits; `--keep` leaves it for inspection, `--dry-run` prints the remote
script, `--ref` proves a specific pushed ref. The script needs GNU `base64`
(Linux, WSL or Git Bash).
