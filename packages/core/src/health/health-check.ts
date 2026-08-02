export function healthCheckV1(): "OK" {
  return "OK";
}

export function healthCheckV2() {
  const now = new Date();
  return {
    status: "OK" as const,
    date: now.toLocaleString("pt-BR", { timeZoneName: "short" }),
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    timezoneOffset: now.getTimezoneOffset(),
    timezoneName: Intl.DateTimeFormat().resolvedOptions().timeZone,
    version: "2.0.0",
    uptime: process.uptime(),
    memory: process.memoryUsage(),
    cpu: process.cpuUsage(),
    os: process.platform,
    arch: process.arch,
    browser: process.browser,
    node: process.version,
    args: process.argv,
  };
}
