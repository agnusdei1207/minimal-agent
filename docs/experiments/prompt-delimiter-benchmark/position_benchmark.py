import os
import re
import json
import time
import requests

API_KEY = os.environ.get("OPENROUTER_API_KEY", "")
MODEL_NAME = "minimax/minimax-m3:free"
API_URL = "https://openrouter.ai/api/v1/chat/completions"

HEADERS = {
    "Authorization": f"Bearer {API_KEY}",
    "Content-Type": "application/json",
    "HTTP-Referer": "https://github.com/agnusdei1207/minimal-agent",
    "X-Title": "Prompt-Delimiter-Position-Benchmark"
}

DELIMITERS = ["xml", "markdown", "square_bracket", "plain_colon", "json"]
POSITIONS = ["head", "middle", "tail"]

def call_openrouter(prompt: str, max_retries: int = 3) -> str:
    payload = {
        "model": MODEL_NAME,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.1,
        "max_tokens": 1000
    }
    for attempt in range(max_retries):
        try:
            resp = requests.post(API_URL, headers=HEADERS, json=payload, timeout=40)
            if resp.status_code == 200:
                data = resp.json()
                return data["choices"][0]["message"]["content"].strip()
            elif resp.status_code == 429:
                time.sleep(2 * (attempt + 1))
            else:
                time.sleep(1)
        except Exception:
            time.sleep(2)
    return ""

def generate_haystack_blocks(n_blocks=12):
    blocks = []
    services = ["auth-service", "billing-gateway", "telemetry-collector", "cache-node-01", "database-replica", "event-broker"]
    for i in range(n_blocks):
        srv = services[i % len(services)]
        text = f"LogRecord-{i:03d}: [2026-09-02 10:{i:02d}:00] {srv} status HEALTHY. Memory usage {30 + (i*3)%50}%, CPU load 0.{i%9}1. Active worker threads {4 + i%8}. Session count {120 + i*15}. Inbound TLS handshake verified."
        blocks.append(text)
    return blocks

def build_position_prompt(delim: str, position: str):
    needle_val = "FLAG{needle_found_7721}"
    inst_text = "Scan the system audit log. Find the hidden master authorization token and output it formatted strictly as 'VERIFIED_TOKEN: <token>'. Do not output anything else."
    
    haystack = generate_haystack_blocks(12) # 12개 블록
    
    if delim == "xml":
        needle_block = f"<auth_override>\nMaster Token: {needle_val}\n</auth_override>"
    elif delim == "markdown":
        needle_block = f"### Auth Override\nMaster Token: {needle_val}"
    elif delim == "square_bracket":
        needle_block = f"[AUTH_OVERRIDE]\nMaster Token: {needle_val}\n[/AUTH_OVERRIDE]"
    elif delim == "plain_colon":
        needle_block = f"Auth Override:\nMaster Token: {needle_val}"
    elif delim == "json":
        needle_block = json.dumps({"auth_override": {"master_token": needle_val}})

    if position == "head":
        full_content_blocks = [needle_block] + haystack
    elif position == "middle":
        mid = len(haystack) // 2
        full_content_blocks = haystack[:mid] + [needle_block] + haystack[mid:]
    elif position == "tail":
        full_content_blocks = haystack + [needle_block]

    full_log_text = "\n\n".join(full_content_blocks)

    if delim == "xml":
        prompt = f"<system_instruction>\n{inst_text}\n</system_instruction>\n<audit_logs>\n{full_log_text}\n</audit_logs>"
    elif delim == "markdown":
        prompt = f"### System Instruction\n{inst_text}\n\n### Audit Logs\n{full_log_text}"
    elif delim == "square_bracket":
        prompt = f"[SYSTEM_INSTRUCTION]\n{inst_text}\n[/SYSTEM_INSTRUCTION]\n[AUDIT_LOGS]\n{full_log_text}\n[/AUDIT_LOGS]"
    elif delim == "plain_colon":
        prompt = f"System Instruction:\n{inst_text}\n\nAudit Logs:\n{full_log_text}"
    elif delim == "json":
        prompt = json.dumps({"system_instruction": inst_text, "audit_logs": full_log_text})

    def evaluate(output: str) -> float:
        if not output:
            return 0.0
        score = 0.0
        cleaned = output.strip()
        # 1. Needle retrieval check (50%)
        if needle_val in cleaned:
            score += 50.0
        # 2. Strict format adherence 'VERIFIED_TOKEN: FLAG{...}' (50%)
        expected = f"VERIFIED_TOKEN: {needle_val}"
        if expected in cleaned:
            score += 50.0
        elif cleaned.startswith("VERIFIED_TOKEN:"):
            score += 25.0
        return score

    return prompt, evaluate

def main():
    print(f"[*] Starting Needle Position (Head/Middle/Tail) Benchmark on model: {MODEL_NAME}")
    os.makedirs("data", exist_ok=True)
    
    REPEATS = 3
    results = []
    total_calls = len(POSITIONS) * len(DELIMITERS) * REPEATS
    done_calls = 0
    
    for pos in POSITIONS:
        for delim in DELIMITERS:
            for rep in range(REPEATS):
                prompt, evaluator = build_position_prompt(delim, pos)
                t0 = time.time()
                response = call_openrouter(prompt)
                latency = time.time() - t0
                score = evaluator(response)
                
                record = {
                    "position": pos,
                    "delimiter": delim,
                    "repeat": rep + 1,
                    "score": score,
                    "latency": latency,
                    "prompt": prompt,
                    "response": response
                }
                results.append(record)
                done_calls += 1
                print(f"[{done_calls}/{total_calls}] Pos: {pos:<6} | {delim:<14} | rep {rep+1} | Score: {score:>5.1f} | Latency: {latency:.2f}s", flush=True)
                time.sleep(0.5)

    with open("data/position_raw_results.json", "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
        
    summary = {}
    for pos in POSITIONS:
        summary[pos] = {}
        for delim in DELIMITERS:
            p_d_records = [r for r in results if r["position"] == pos and r["delimiter"] == delim]
            avg = sum(r["score"] for r in p_d_records) / len(p_d_records)
            summary[pos][delim] = round(avg, 2)
            
    with open("data/position_summary_metrics.json", "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)
        
    print("\n[+] Position Benchmark Complete. Summary:")
    print(json.dumps(summary, indent=2))

if __name__ == "__main__":
    main()
