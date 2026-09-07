import os
import requests
import re

TOKEN = os.environ.get("GHCR_TOKEN")
REPO = "agnusdei1207/minimal-agent-pentesting"
RELEASE_ID = 380917952
TAG_NAME = "benchmark-assets-v1"

HEADERS_API = {
    "Authorization": f"Bearer {TOKEN}",
    "Accept": "application/vnd.github+json"
}

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))

FILES = [
    ("fig1_baseline_vs_stress.png", os.path.join(SCRIPT_DIR, "figures", "fig1_baseline_vs_stress.png")),
    ("fig2_stress_breakdown.png", os.path.join(SCRIPT_DIR, "figures", "fig2_stress_breakdown.png")),
    ("fig3_position_sensitivity.png", os.path.join(SCRIPT_DIR, "figures", "fig3_position_sensitivity.png")),
]

def upload_assets():
    cdn_urls = {}
    for filename, filepath in FILES:
        if not os.path.exists(filepath):
            print(f"[-] File not found: {filepath}")
            continue
            
        url = f"https://uploads.github.com/repos/{REPO}/releases/{RELEASE_ID}/assets?name={filename}"
        with open(filepath, "rb") as f:
            data = f.read()
            
        headers = {
            "Authorization": f"Bearer {TOKEN}",
            "Content-Type": "image/png"
        }
        resp = requests.post(url, headers=headers, data=data)
        if resp.status_code in [200, 201]:
            asset_info = resp.json()
            download_url = asset_info["browser_download_url"]
            print(f"[+] Uploaded {filename} -> {download_url}")
            cdn_urls[filename] = download_url
        elif resp.status_code == 422: # already exists
            download_url = f"https://github.com/{REPO}/releases/download/{TAG_NAME}/{filename}"
            print(f"[!] Already uploaded {filename} -> {download_url}")
            cdn_urls[filename] = download_url
        else:
            print(f"[-] Error uploading {filename}: {resp.status_code} {resp.text}")
    return cdn_urls

def update_paper_md(cdn_urls):
    paper_path = os.path.join(SCRIPT_DIR, "PAPER.md")
    with open(paper_path, "r", encoding="utf-8") as f:
        content = f.read()
        
    for filename, cdn_url in cdn_urls.items():
        content = re.sub(rf'\(figures/{filename}\)', f'({cdn_url})', content)
        content = re.sub(rf'\(\.\./\.\./experiments/prompt-delimiter-benchmark/figures/{filename}\)', f'({cdn_url})', content)
        
    with open(paper_path, "w", encoding="utf-8") as f:
        f.write(content)
        
    print("[+] Updated PAPER.md with global CDN image URLs.")
    return content

def create_and_close_issue(body_content):
    issue_payload = {
        "title": "[Research Report] Prompt Delimiter Syntax and Positional Brittleness in Modern LLMs",
        "body": body_content,
        "labels": ["research", "benchmark", "documentation"]
    }
    
    # 1. Create Issue
    r = requests.post(f"https://api.github.com/repos/{REPO}/issues", headers=HEADERS_API, json=issue_payload)
    if r.status_code not in [200, 201]:
        print(f"[-] Failed to create issue: {r.status_code} {r.text}")
        return None
        
    issue_data = r.json()
    issue_num = issue_data["number"]
    issue_url = issue_data["html_url"]
    print(f"[+] Successfully Created Issue #{issue_num}: {issue_url}")
    
    # 2. Close Issue
    close_payload = {
        "state": "closed",
        "state_reason": "completed"
    }
    r_close = requests.patch(f"https://api.github.com/repos/{REPO}/issues/{issue_num}", headers=HEADERS_API, json=close_payload)
    if r_close.status_code == 200:
        print(f"[+] Successfully Closed Issue #{issue_num} (state_reason: completed)")
    else:
        print(f"[-] Failed to close issue: {r_close.status_code} {r_close.text}")
        
    return issue_url

def main():
    print("[*] Uploading PNG figures to GitHub Release Assets...")
    cdn_urls = upload_assets()
    
    print("[*] Updating PAPER.md with CDN URLs...")
    updated_paper = update_paper_md(cdn_urls)
    
    print("[*] Creating and closing research report Issue...")
    issue_url = create_and_close_issue(updated_paper)
    
    print(f"\n[★] All done! Permanent Issue URL: {issue_url}")

if __name__ == "__main__":
    main()
