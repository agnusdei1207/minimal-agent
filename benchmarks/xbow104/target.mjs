const INFRA_SERVICES = new Set([
  "db", "mysql", "postgres", "mariadb", "redis", "minio", "mailpit", "database",
]);

const tcp = (port) => port && (!port.Protocol || String(port.Protocol).toLowerCase() === "tcp");
const validPort = (port) => Number.isInteger(Number(port)) && Number(port) > 0 && Number(port) <= 65535;

export function targetDiscoveryError(message) {
  const error = new Error(`target discovery failed: ${message}`);
  error.code = "BENCHMARK_TARGET_DISCOVERY";
  return error;
}

export function selectTargetService(names, rows) {
  const apps = names.filter((name) => !INFRA_SERVICES.has(name.toLowerCase()));
  // Compose can list dependencies before the public entrypoint (XBEN-020).
  // Keep expose-only services private; publishing them would bypass the SSRF.
  const service = apps.find((name) => rows.some((row) =>
    row?.Service === name && Array.isArray(row.Publishers) &&
    row.Publishers.some((port) => tcp(port) && validPort(port.PublishedPort)),
  )) || apps[0] || names[0];
  if (!service || !rows.some((row) => row?.Service === service))
    throw targetDiscoveryError(`no Compose state for selected service ${service || "(none)"}`);
  return service;
}

/** Resolve an HTTP endpoint without publishing or bypassing private services. */
export function resolveTargetEndpoint(service, row) {
  if (!service || row?.Service !== service)
    throw targetDiscoveryError(`no Compose state for selected service ${service || "(none)"}`);
  const publishers = Array.isArray(row.Publishers) ? row.Publishers : [];
  const ports = publishers.filter(tcp);
  if (publishers.length && !ports.length)
    throw targetDiscoveryError(`service ${service} has no TCP endpoint`);
  const published = ports.find((port) => validPort(port.PublishedPort));
  const exposed = ports.find((port) => validPort(port.TargetPort));
  const internalPort = Number(published?.TargetPort ?? exposed?.TargetPort ?? row.ServicePort ?? row.TargetPort ?? 80);
  if (!validPort(internalPort)) throw targetDiscoveryError(`invalid TCP port for ${service}`);
  return {
    service,
    published_port: published ? Number(published.PublishedPort) : null,
    hostUrl: published ? `http://127.0.0.1:${published.PublishedPort}` : null,
    internalUrl: `http://${service}:${internalPort}`,
    internalPort,
  };
}
