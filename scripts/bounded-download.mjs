export async function readResponseBounded(response, maxBytes, label) {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 1) {
    throw new TypeError("maxBytes must be a positive safe integer");
  }

  const declared = response.headers.get("content-length");
  if (declared !== null) {
    const normalized = declared.trim();
    if (!/^\d+$/.test(normalized) || !Number.isSafeInteger(Number(normalized))) {
      throw new Error(`${label} has an invalid Content-Length`);
    }
    if (Number(normalized) > maxBytes) {
      throw new Error(`${label} exceeds ${maxBytes} bytes`);
    }
  }

  if (!response.body) {
    throw new Error(`${label} response has no body`);
  }

  const reader = response.body.getReader();
  const chunks = [];
  let total = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      total += value.byteLength;
      if (total > maxBytes) {
        await reader.cancel(`${label} exceeds its byte envelope`).catch(() => {});
        throw new Error(`${label} exceeds ${maxBytes} bytes`);
      }
      chunks.push(Buffer.from(value));
    }
  } finally {
    reader.releaseLock();
  }
  return Buffer.concat(chunks, total);
}
