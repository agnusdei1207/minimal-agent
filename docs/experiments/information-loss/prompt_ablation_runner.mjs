import { readFile, writeFile } from 'node:fs/promises';

const OPENROUTER_API_KEY = process.env.OPENROUTER_API_KEY;
const MODEL = process.env.OPENROUTER_MODEL || 'minimax/minimax-m3:free';
const ENDPOINT = 'https://openrouter.ai/api/v1/chat/completions';

async function callModel(messages, temperature = 0.1) {
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const res = await fetch(ENDPOINT, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${OPENROUTER_API_KEY}`
        },
        body: JSON.stringify({
          model: MODEL,
          messages,
          temperature,
          stream: false
        })
      });
      if (!res.ok) {
        const errText = await res.text();
        throw new Error(`HTTP ${res.status}: ${errText}`);
      }
      const json = await res.json();
      const text = json.choices?.[0]?.message?.content || '';
      const usage = json.usage || { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 };
      return { text, usage };
    } catch (e) {
      if (attempt === 3) throw e;
      await new Promise(r => setTimeout(r, 2000 * attempt));
    }
  }
}

const PROMPT_STRATEGIES = {
  P1_Simple: {
    name: "P1: 단순 요약 (Naive Summary)",
    system: "You are an intermediate team lead in an autonomous security team. Summarize the worker report concisely for your main manager:"
  },
  P2_StructuredKV: {
    name: "P2: 구조화된 3단 템플릿 (Structured Key-Value)",
    system: `You are an intermediate lead. Format the update into exactly 3 sections:
[STATUS]: 1-sentence progress summary
[VERBATIM_FACTS]: Bulleted list preserving ALL exact offsets, hex addresses (0x...), URLs, parameters, payloads, hashes, credentials, and flags without altering a single character.
[NEXT_MOVE]: 1-sentence next action`
  },
  P3_StrictJSON: {
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

async function runAblation() {
  const datasetPath = new URL('./dataset.json', import.meta.url);
  const dataset = JSON.parse(await readFile(datasetPath, 'utf8'));

  console.log(`================================================================`);
  console.log(`🧪 PROMPT STRATEGY ABLATION EXPERIMENT (5 Strategies × 10 Scenarios)`);
  console.log(`================================================================\n`);

  const results = {
    strategies: {},
    scenario_details: []
  };

  for (const [sKey, sObj] of Object.entries(PROMPT_STRATEGIES)) {
    results.strategies[sKey] = {
      name: sObj.name,
      total_preserved: 0,
      total_facts: 0,
      avg_preservation_rate: 0,
      exploitability_count: 0,
      total_tokens: 0
    };
  }

  for (const scenario of dataset) {
    console.log(`▶ Scenario: ${scenario.name} (${scenario.id})`);
    const scenarioRecord = {
      id: scenario.id,
      name: scenario.name,
      facts_count: Object.keys(scenario.raw_facts).length,
      runs: {}
    };

    for (const [sKey, sObj] of Object.entries(PROMPT_STRATEGIES)) {
      // Intermediate lead turn
      const leadRes = await callModel([
        { role: 'system', content: sObj.system },
        { role: 'user', content: scenario.leaf_report }
      ]);

      // Root extraction turn (evaluating what Root coordinator actually receives)
      const rootRes = await callModel([
        {
          role: 'system',
          content: 'You are the main coordinator. Extract all specific technical facts (offsets, hex addresses, URLs, parameters, payloads, credentials, hashes, flags) from the message below and return them as a JSON object mapping key_name to value:'
        },
        { role: 'user', content: `Message from intermediate lead:\n${leadRes.text}` }
      ]);

      const evalResult = evaluateFacts(scenario.raw_facts, `${leadRes.text}\n${rootRes.text}`);
      const tokens = leadRes.usage.total_tokens + rootRes.usage.total_tokens;

      scenarioRecord.runs[sKey] = {
        lead_output: leadRes.text,
        root_extraction: rootRes.text,
        metrics: evalResult,
        tokens
      };

      results.strategies[sKey].total_preserved += evalResult.preserved;
      results.strategies[sKey].total_facts += evalResult.total;
      results.strategies[sKey].avg_preservation_rate += evalResult.preservationRate;
      results.strategies[sKey].exploitability_count += evalResult.exploitability;
      results.strategies[sKey].total_tokens += tokens;

      console.log(`   - [${sKey}] Preserved: ${evalResult.preserved}/${evalResult.total} (${evalResult.preservationRate.toFixed(1)}%) | Exploit: ${evalResult.exploitability} | Tokens: ${tokens}`);
      await new Promise(r => setTimeout(r, 600)); // rate limit buffer
    }
    results.scenario_details.push(scenarioRecord);
    console.log('');
  }

  for (const sKey of Object.keys(PROMPT_STRATEGIES)) {
    const s = results.strategies[sKey];
    s.avg_preservation_rate = (s.avg_preservation_rate / dataset.length).toFixed(2);
    s.exploitability_rate = ((s.exploitability_count / dataset.length) * 100).toFixed(2);
    s.avg_tokens_per_scenario = Math.round(s.total_tokens / dataset.length);
  }

  const outPath = new URL('./prompt_ablation_results.json', import.meta.url);
  await writeFile(outPath, JSON.stringify(results, null, 2));

  console.log(`================================================================`);
  console.log(`🏁 PROMPT STRATEGY ABLATION COMPLETED`);
  console.log(`================================================================`);
  console.log(JSON.stringify(results.strategies, null, 2));
}

runAblation().catch(console.error);
