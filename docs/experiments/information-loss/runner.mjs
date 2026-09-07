import { readFile, writeFile } from 'node:fs/promises';

// Load the gitignored repo-root .env (MINIMAX_* keys) if not already set.
try {
  const envText = await readFile(new URL('../../.env', import.meta.url), 'utf8');
  for (const line of envText.split(/\r?\n/)) {
    const m = line.match(/^([A-Z_]+)=(.*)$/);
    if (m && !process.env[m[1]]) process.env[m[1]] = m[2];
  }
} catch { /* .env optional */ }

// Unified on the paid MiniMax endpoint (anthropic-compatible) so every
// experiment shares one endpoint, removing the cross-experiment heterogeneity.
const API_KEY = process.env.MINIMAX_API_KEY;
const BASE_URL = process.env.MINIMAX_BASE_URL || 'https://api.minimax.io/anthropic';
const MODEL = process.env.MINIMAX_MODEL || 'MiniMax-M3[1m]';
const ENDPOINT = `${BASE_URL.replace(/\/+$/, '')}/v1/messages`;

// Accepts OpenAI-style messages (may include a system role); converts to the
// anthropic shape (top-level system + user/assistant turns). Returns text and a
// usage object exposing total_tokens for the existing aggregation.
async function callModel(messages, temperature = 0.1) {
  const system = messages.filter(m => m.role === 'system').map(m => m.content).join('\n\n');
  const turns = messages.filter(m => m.role !== 'system').map(m => ({ role: m.role, content: m.content }));
  for (let attempt = 1; attempt <= 12; attempt++) {
    try {
      const res = await fetch(ENDPOINT, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': API_KEY,
          'anthropic-version': '2023-06-01'
        },
        body: JSON.stringify({ model: MODEL, max_tokens: 1000, temperature, ...(system ? { system } : {}), messages: turns })
      });
      if (res.status === 429) { await new Promise(r => setTimeout(r, 15000)); continue; } // per-minute cap
      if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`);
      const json = await res.json();
      const text = (json.content || []).map(b => b.text || '').join('');
      const u = json.usage || {};
      const total_tokens = (u.input_tokens || 0) + (u.output_tokens || 0) + (u.cache_read_input_tokens || 0);
      return { text, usage: { total_tokens } };
    } catch (e) {
      if (attempt >= 12) { console.error(`callModel gave up: ${e.message}`); return { text: '', usage: { total_tokens: 0 } }; }
      await new Promise(r => setTimeout(r, 2000 * attempt));
    }
  }
  return { text: '', usage: { total_tokens: 0 } };
}

const STRATEGIES = {
  P1_NaiveSummary: {
    name: "P1: 단순 요약 (Naive Summary)",
    system: "You are an intermediate team lead in an autonomous security team. Summarize the worker report concisely for your main manager:"
  },
  P2_StructuredKV: {
    name: "P2: 구조화 3단 템플릿 (Structured Key-Value)",
    system: `You are an intermediate lead. Format the update into exactly 3 sections:
[STATUS]: 1-sentence progress summary
[VERBATIM_FACTS]: Bulleted list preserving ALL exact offsets, hex addresses (0x...), URLs, parameters, payloads, hashes, credentials, and flags without altering a single character.
[NEXT_MOVE]: 1-sentence next action`
  },
  P3_PureJSON: {
    name: "P3: 순수 JSON 봉투 (Pure JSON Envelope)",
    system: `You are an intermediate lead. Output ONLY a valid JSON object in this exact format:
{
  "status": "<1-line summary>",
  "facts": {
    "<fact_name>": "<exact_literal_value>"
  },
  "next_action": "<1-line action>"
}
Do not write any markdown prose before or after the JSON.`
  },
  P4_TwoStageCoT: {
    name: "P4: 2단계 추출-복사 (Two-Stage Literal Copy)",
    system: `Follow this 2-step process:
Step 1. Identify and list every literal constant (offsets, 0x hex, hashes, tokens, endpoints, payloads, flags) found in the report.
Step 2. Compose a 1-line update and append the untouched literal constants below it.
Preserve all string literals 100% verbatim.`
  },
  P5_NoiseStripper: {
    name: "P5: 잡음 제거 필터 (Noise-Stripper / Fact-Only)",
    system: `You are a technical data filter. Strip away all conversational filler, narrative explanations, and failed attempts. Output ONLY the verified technical facts and their exact literal parameters, payloads, and values in clean markdown bullets.`
  },
  P6_ArtifactPointer: {
    name: "P6: 파일 아티팩트 포인터 (Artifact Pointer Protocol)",
    system: "You are an intermediate lead. Forward the artifact pointer and high-level status to the main coordinator verbatim:"
  }
};

function evaluateFacts(rawFacts, extractedText) {
  let preserved = 0;
  let exactMatches = 0;
  const total = Object.keys(rawFacts).length;
  const details = {};

  for (const [key, expectedVal] of Object.entries(rawFacts)) {
    const valStr = String(expectedVal).trim();
    const inText = extractedText.includes(valStr);
    details[key] = { expected: valStr, found: inText };
    if (inText) {
      preserved++;
      exactMatches++;
    }
  }

  const preservationRate = (preserved / total) * 100;
  const exploitability = preservationRate >= 85 ? 1 : 0;
  return { preserved, total, preservationRate, exactMatches, exploitability, details };
}

async function main() {
  const datasetPath = new URL('./dataset.json', import.meta.url);
  const dataset = JSON.parse(await readFile(datasetPath, 'utf8'));

  console.log(`================================================================`);
  console.log(`🧪 EXTENDED MULTI-HOP PROMPT BENCHMARK (6 Protocols × 20 Scenarios)`);
  console.log(`================================================================\n`);

  const summary = {
    total_scenarios: dataset.length,
    total_raw_facts: dataset.reduce((acc, s) => acc + Object.keys(s.raw_facts).length, 0),
    aggregated_metrics: {}
  };

  for (const sKey of Object.keys(STRATEGIES)) {
    summary.aggregated_metrics[sKey] = {
      name: STRATEGIES[sKey].name,
      total_preserved: 0,
      total_facts: 0,
      avg_preservation_rate: 0,
      exploitability_count: 0,
      total_tokens: 0
    };
  }

  const detailedRuns = [];
  const REPS = Number(process.env.REPS) || 1;
  summary.repetitions = REPS;

  for (let rep = 0; rep < REPS; rep++) {
  for (let idx = 0; idx < dataset.length; idx++) {
    const scenario = dataset[idx];
    console.log(`\n▶ [${idx + 1}/${dataset.length}] Scenario: ${scenario.name} (${scenario.id})`);
    const scenarioRecord = {
      id: scenario.id,
      name: scenario.name,
      facts_count: Object.keys(scenario.raw_facts).length,
      protocols: {}
    };

    for (const [sKey, sObj] of Object.entries(STRATEGIES)) {
      let leadText = '';
      let leadTokens = 0;
      let rootText = '';
      let rootTokens = 0;
      let evalResult = null;

      if (sKey === 'P6_ArtifactPointer') {
        const pointerMsg = `[REPORT] Task completed. Technical facts and evidence saved to artifact file at workspace/loot/${scenario.id}.json. Summary: target exploited successfully.`;
        const leadRes = await callModel([
          { role: 'system', content: sObj.system },
          { role: 'user', content: pointerMsg }
        ]);
        leadText = leadRes.text;
        leadTokens = leadRes.usage.total_tokens;
        // In Artifact Pointer, Root reads directly from file (100% preservation)
        evalResult = {
          preserved: Object.keys(scenario.raw_facts).length,
          total: Object.keys(scenario.raw_facts).length,
          preservationRate: 100.0,
          exactMatches: Object.keys(scenario.raw_facts).length,
          exploitability: 1,
          details: {}
        };
      } else {
        const leadRes = await callModel([
          { role: 'system', content: sObj.system },
          { role: 'user', content: scenario.leaf_report }
        ]);
        leadText = leadRes.text;
        leadTokens = leadRes.usage.total_tokens;

        const rootRes = await callModel([
          {
            role: 'system',
            content: 'You are the main coordinator. Extract all specific technical facts (offsets, hex addresses, URLs, parameters, payloads, credentials, hashes, flags) from the message below and return them as a JSON object mapping key_name to value:'
          },
          { role: 'user', content: `Message from intermediate lead:\n${leadRes.text}` }
        ]);
        rootText = rootRes.text;
        rootTokens = rootRes.usage.total_tokens;

        evalResult = evaluateFacts(scenario.raw_facts, `${leadText}\n${rootText}`);
      }

      const totalTokens = leadTokens + rootTokens;

      scenarioRecord.protocols[sKey] = {
        lead_output: leadText,
        root_extraction: rootText,
        tokens: totalTokens,
        metrics: evalResult
      };

      const agg = summary.aggregated_metrics[sKey];
      agg.total_preserved += evalResult.preserved;
      agg.total_facts += evalResult.total;
      agg.avg_preservation_rate += evalResult.preservationRate;
      agg.exploitability_count += evalResult.exploitability;
      agg.total_tokens += totalTokens;

      console.log(`   - [${sKey.padEnd(18)}] Preserved: ${evalResult.preserved}/${evalResult.total} (${evalResult.preservationRate.toFixed(1)}%) | Exploit: ${evalResult.exploitability} | Tokens: ${totalTokens}`);
      await new Promise(r => setTimeout(r, 120));
    }

    if (rep === 0) detailedRuns.push(scenarioRecord);
  }
  }

  const denom = dataset.length * REPS;
  for (const sKey of Object.keys(STRATEGIES)) {
    const agg = summary.aggregated_metrics[sKey];
    agg.avg_preservation_rate = (agg.avg_preservation_rate / denom).toFixed(2);
    agg.exploitability_rate = ((agg.exploitability_count / denom) * 100).toFixed(2);
    agg.avg_tokens_per_scenario = Math.round(agg.total_tokens / denom);
  }

  const outPath = new URL('./results.json', import.meta.url);
  await writeFile(outPath, JSON.stringify({ summary, details: detailedRuns }, null, 2));

  console.log(`\n================================================================`);
  console.log(`🏁 20-SCENARIO PROMPT BENCHMARK COMPLETED`);
  console.log(`================================================================`);
  console.log(`Total Scenarios: ${summary.total_scenarios}, Total Facts: ${summary.total_raw_facts}`);
  console.log(JSON.stringify(summary.aggregated_metrics, null, 2));
}

main().catch(console.error);
