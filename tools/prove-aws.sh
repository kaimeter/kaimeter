#!/usr/bin/env bash
#
# Run the ignored release measurements on an on-demand EC2 instance.
#
# The instance is launched from the latest Ubuntu 24.04 AMI, driven through
# SSM Session Manager (no SSH key or inbound rule), and terminated when the
# script exits. Credentials and region follow the AWS CLI chain: profiles,
# SSO, environment variables or an instance role. See tools/README.md for
# the one-time instance-profile setup.
#
# Usage: tools/prove-aws.sh --instance-profile NAME [options]
#        tools/prove-aws.sh --help

set -euo pipefail

REPO_URL="https://github.com/kaimeter/kaimeter.git"
DEFAULT_TYPE="c6i.2xlarge"
DEFAULT_NAME="kaimeter-prove"

usage() {
    cat <<'USAGE'
Usage: tools/prove-aws.sh --instance-profile NAME [options]

Options:
  --instance-profile NAME   IAM instance profile carrying
                            AmazonSSMManagedInstanceCore (required; or set
                            PROVE_INSTANCE_PROFILE)
  --ref REF                 Branch or commit the instance proves
                            (default: the current branch, must be pushed)
  --instance-type TYPE      EC2 instance type (default: c6i.2xlarge)
  --region REGION           AWS region (default: AWS_REGION/AWS_DEFAULT_REGION)
  --keep                    Do not terminate the instance on exit
  --dry-run                 Print the remote script and exit
  -h, --help                Print this help

The script runs `cargo test --test aluminium -- --ignored --nocapture` in
crates/kaimeter-guest on the instance and prints the command output, which
carries the cycle counts, proving time and image ID.
USAGE
}

die() {
    echo "prove-aws: $*" >&2
    exit 1
}

TYPE="$DEFAULT_TYPE"
NAME="$DEFAULT_NAME"
REGION="${AWS_REGION:-${AWS_DEFAULT_REGION:-}}"
PROFILE="${PROVE_INSTANCE_PROFILE:-}"
REF=""
KEEP=0
DRY_RUN=0

while [ $# -gt 0 ]; do
    case "$1" in
        --instance-profile) PROFILE="${2:-}"; shift 2 ;;
        --ref) REF="${2:-}"; shift 2 ;;
        --instance-type) TYPE="${2:-}"; shift 2 ;;
        --region) REGION="${2:-}"; shift 2 ;;
        --keep) KEEP=1; shift ;;
        --dry-run) DRY_RUN=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown argument '$1' (try --help)" ;;
    esac
done

command -v git >/dev/null 2>&1 || die "git not found"
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || die "not in a git repository"

if [ -z "$REF" ]; then
    REF=$(git rev-parse --abbrev-ref HEAD)
    [ "$REF" != "HEAD" ] || die "detached HEAD; pass --ref"
    [ -z "$(git status --porcelain)" ] || die "working tree is dirty; commit and push, or pass --ref"
    git fetch --quiet origin "$REF" || die "cannot fetch origin"
    [ "$(git rev-parse HEAD)" = "$(git rev-parse "origin/$REF")" ] ||
        die "branch '$REF' is not pushed to origin"
else
    case "$REF" in
        *[!A-Za-z0-9._/-]*) die "invalid ref '$REF'" ;;
    esac
    [ -z "$(git status --porcelain)" ] ||
        echo "prove-aws: warning: local changes are not proved; running $REF from origin" >&2
fi

[ -n "$REGION" ] || die "no region; pass --region or set AWS_REGION"
[ -n "$PROFILE" ] || die "no instance profile; pass --instance-profile or set PROVE_INSTANCE_PROFILE
One-time setup:
  aws iam create-role --role-name kaimeter-prove --assume-role-policy-document file://trust.json
  aws iam attach-role-policy --role-name kaimeter-prove \
    --policy-arn arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore
  aws iam create-instance-profile --instance-profile-name kaimeter-prove
  aws iam add-role-to-instance-profile \
    --instance-profile-name kaimeter-prove --role-name kaimeter-prove"

REMOTE_SCRIPT=$(cat <<'REMOTE_EOF'
set -euo pipefail

echo "== kaimeter prove: ref $REF"
started=$(date +%s)

export DEBIAN_FRONTEND=noninteractive
echo "== installing build prerequisites"
sudo apt-get update -qq
sudo apt-get install -y -qq build-essential pkg-config libssl-dev curl git

if ! command -v cargo >/dev/null 2>&1; then
  echo "== installing rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup.sh
  sh /tmp/rustup.sh -y --profile minimal --component rustfmt >/dev/null
fi
export PATH="$HOME/.cargo/bin:$PATH"

if [ ! -x "$HOME/.risc0/bin/rzup" ]; then
  echo "== installing rzup"
  curl -L https://risczero.com/install -o /tmp/rzup-install.sh
  bash /tmp/rzup-install.sh >/dev/null
fi
echo "== installing the pinned RISC Zero toolchain"
"$HOME/.risc0/bin/rzup" install rust 1.97.0 >/dev/null
"$HOME/.risc0/bin/rzup" install r0vm 3.0.6 >/dev/null
export PATH="$HOME/.risc0/bin:$PATH"

echo "== cloning $REPO_URL at $REF"
rm -rf "$HOME/kaimeter"
git clone --quiet "$REPO_URL" "$HOME/kaimeter"
cd "$HOME/kaimeter"
git checkout --quiet "$REF"

echo "== running the release measurements"
cd crates/kaimeter-guest
status=0
cargo test --test aluminium -- --ignored --nocapture >"$HOME/prove.out" 2>&1 || status=$?
tail -60 "$HOME/prove.out"
echo "== wall seconds: $(( $(date +%s) - started ))"
exit "$status"
REMOTE_EOF
)
REMOTE_SCRIPT="REF='$REF'
$REMOTE_SCRIPT"

if [ "$DRY_RUN" = "1" ]; then
    echo "instance type: $TYPE"
    echo "region:        $REGION"
    echo "profile:       $PROFILE"
    echo "--- remote script ---"
    printf '%s\n' "$REMOTE_SCRIPT"
    exit 0
fi

command -v aws >/dev/null 2>&1 || die "aws CLI not found"
command -v base64 >/dev/null 2>&1 || die "base64 not found"

AWS=(aws --region "$REGION")

echo "== resolving the latest Ubuntu 24.04 AMI"
AMI=$("${AWS[@]}" ssm get-parameter \
    --name /aws/service/canonical/ubuntu/server/24.04/stable/current/amd64/hvm/ebs-gp3/ami-id \
    --query 'Parameter.Value' --output text)

echo "== resolving the default subnet and security group"
SUBNET=$("${AWS[@]}" ec2 describe-subnets \
    --filters Name=default-for-az,Values=true \
    --query 'Subnets[0].SubnetId' --output text)
VPC=$("${AWS[@]}" ec2 describe-subnets --subnet-ids "$SUBNET" \
    --query 'Subnets[0].VpcId' --output text)
SG=$("${AWS[@]}" ec2 describe-security-groups \
    --filters "Name=vpc-id,Values=$VPC" Name=group-name,Values=default \
    --query 'SecurityGroups[0].GroupId' --output text)

INSTANCE=""
terminate() {
    if [ -n "$INSTANCE" ] && [ "$KEEP" = "0" ]; then
        echo "== terminating $INSTANCE"
        "${AWS[@]}" ec2 terminate-instances --instance-ids "$INSTANCE" >/dev/null || true
    fi
}
trap terminate EXIT

echo "== launching $TYPE"
INSTANCE=$("${AWS[@]}" ec2 run-instances \
    --image-id "$AMI" \
    --instance-type "$TYPE" \
    --iam-instance-profile "Name=$PROFILE" \
    --subnet-id "$SUBNET" \
    --security-group-ids "$SG" \
    --instance-initiated-shutdown-behavior terminate \
    --tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=$NAME}]" \
    --query 'Instances[0].InstanceId' --output text)
echo "== instance $INSTANCE"
if [ "$KEEP" = "1" ]; then
    echo "== keeping the instance; terminate it yourself when done"
fi

"${AWS[@]}" ec2 wait instance-running --instance-ids "$INSTANCE"

echo "== waiting for SSM"
for _ in $(seq 1 60); do
    PING=$("${AWS[@]}" ssm describe-instance-information \
        --filters "Key=InstanceIds,Values=$INSTANCE" \
        --query 'InstanceInformationList[0].PingStatus' --output text 2>/dev/null || true)
    if [ "$PING" = "Online" ]; then
        break
    fi
    sleep 10
done
[ "$PING" = "Online" ] || die "instance did not register with SSM"

ENCODED=$(printf '%s' "$REMOTE_SCRIPT" | base64 -w0)
COMMAND=$("${AWS[@]}" ssm send-command \
    --instance-ids "$INSTANCE" \
    --document-name AWS-RunShellScript \
    --comment "kaimeter prove $REF" \
    --timeout-seconds 7200 \
    --parameters "commands=echo $ENCODED | base64 -d | bash" \
    --query 'Command.CommandId' --output text)
echo "== command $COMMAND"

"${AWS[@]}" ssm wait command-executed --command-id "$COMMAND" --instance-id "$INSTANCE" || true
STATUS=$("${AWS[@]}" ssm get-command-invocation --command-id "$COMMAND" \
    --instance-id "$INSTANCE" --query 'Status' --output text)
"${AWS[@]}" ssm get-command-invocation --command-id "$COMMAND" \
    --instance-id "$INSTANCE" --query 'StandardOutputContent' --output text
STDERR=$("${AWS[@]}" ssm get-command-invocation --command-id "$COMMAND" \
    --instance-id "$INSTANCE" --query 'StandardErrorContent' --output text)
if [ -n "$STDERR" ]; then
    echo "$STDERR" >&2
fi

[ "$STATUS" = "Success" ] || die "remote command finished with status $STATUS"
echo "== done"
