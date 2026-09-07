variable "APP_TAG" {
  default = "pentesting:0.200.0"
}

target "base" {
  context    = "."
  dockerfile = "docker/runtime-base.Dockerfile"
  tags       = [
    "agnusdei1207/pentesting-runtime-base:latest",
    "agnusdei1207/minimal-agent-runtime-base:latest"
  ]
}

target "app" {
  context    = "."
  dockerfile = "docker/app.Dockerfile"
  args = {
    VERSION = "0.200.0"
  }
  tags = [APP_TAG]
}

target "runner" {
  context    = "."
  dockerfile = "benchmarks/harness/Dockerfile.runner"
  contexts = {
    "pentesting:check"    = "target:app"
    "minimal-agent:check" = "target:app"
  }
  tags = ["xbow-agent-runner:latest"]
}
