variable "APP_TAG" {
  default = "minimal-agent:0.110.0"
}

target "base" {
  context    = "."
  dockerfile = "docker/runtime-base.Dockerfile"
  tags       = ["agnusdei1207/minimal-agent-runtime-base:latest"]
}

target "app" {
  context    = "."
  dockerfile = "docker/app.Dockerfile"
  contexts = {
    runtime-base = "target:base"
  }
  args = {
    VERSION = "0.110.0"
  }
  tags = [APP_TAG]
}

target "runner" {
  context    = "."
  dockerfile = "benchmarks/harness/Dockerfile.runner"
  contexts = {
    "minimal-agent:check" = "target:app"
  }
  tags = ["xbow-agent-runner:latest"]
}
