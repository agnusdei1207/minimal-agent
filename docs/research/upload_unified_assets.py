import os
import re
import requests

TOKEN = os.environ.get("GITHUB_TOKEN")
REPO = "agnusdei1207/pentesting"
TAG_NAME = "unified-prompt-assets-v1"

HEADERS_API = {
    "Authorization": f"Bearer {TOKEN}",
    "Accept": "application/vnd.github+json"
}

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
FIGURES_DIR = os.path.join(SCRIPT_DIR, "figures")

FILES = [
    "fig1_multihop_preservation_and_exploit.png",
    "fig2_delimiter_stress_robustness.png",
    "fig3_position_sensitivity_lost_in_middle.png",
    "fig4_bold_ablation_accuracy.png",
    "fig5_token_consumption_and_savings.png"
]

def get_or_create_release():
    r_get = requests.get(f"https://api.github.com/repos/{REPO}/releases/tags/{TAG_NAME}", headers=HEADERS_API)
    if r_get.status_code == 200:
        rel = r_get.json()
        print(f"[+] Found existing release: {rel['id']}")
        return rel['id']
        
    rel_payload = {
        'tag_name': TAG_NAME,
        'target_commitish': 'main',
        'name': 'Unified Prompt Engineering Study Assets v1',
        'body': 'High-resolution publication figures for Unified Prompt Engineering Study (pentesting)',
        'draft': False,
        'prerelease': False
    }
    r = requests.post(f"https://api.github.com/repos/{REPO}/releases", headers=HEADERS_API, json=rel_payload)
    if r.status_code in [200, 201]:
        rel = r.json()
        print(f"[+] Created release: {rel['id']}")
        return rel['id']
    else:
        print(f"[-] Release creation failed: {r.status_code} {r.text}")
        return None

def upload_assets(release_id):
    cdn_urls = {}
    for filename in FILES:
        filepath = os.path.join(FIGURES_DIR, filename)
        if not os.path.exists(filepath):
            print(f"[-] File not found: {filepath}")
            continue
            
        url = f"https://uploads.github.com/repos/{REPO}/releases/{release_id}/assets?name={filename}"
        with open(filepath, "rb") as f:
            data = f.read()
            
        headers = {
            "Authorization": f"Bearer {TOKEN}",
            "Content-Type": "image/png"
        }
        resp = requests.post(url, headers=headers, data=data)
        if resp.status_code in [200, 201]:
            dl_url = resp.json()["browser_download_url"]
            print(f"[+] Uploaded {filename} -> {dl_url}")
            cdn_urls[filename] = dl_url
        elif resp.status_code == 422: # Already exists
            dl_url = f"https://github.com/{REPO}/releases/download/{TAG_NAME}/{filename}"
            print(f"[!] Already exists {filename} -> {dl_url}")
            cdn_urls[filename] = dl_url
        else:
            print(f"[-] Upload failed for {filename}: {resp.status_code} {resp.text}")
            dl_url = f"https://github.com/{REPO}/releases/download/{TAG_NAME}/{filename}"
            cdn_urls[filename] = dl_url
    return cdn_urls

def update_paper(cdn_urls):
    paper_path = os.path.join(SCRIPT_DIR, "UNIFIED_PROMPT_ENGINEERING_STUDY.md")
    with open(paper_path, "r", encoding="utf-8") as f:
        content = f.read()
        
    for filename, dl_url in cdn_urls.items():
        # Replace relative paths figures/{filename} with permanent CDN url
        content = re.sub(rf'\(figures/{filename}\)', f'({dl_url})', content)
        
    with open(paper_path, "w", encoding="utf-8") as f:
        f.write(content)
    print(f"[+] Updated {paper_path} with permanent CDN URLs")

def main():
    if not TOKEN:
        print("[-] GITHUB_TOKEN is not set")
        return
        
    rel_id = get_or_create_release()
    if not rel_id:
        return
        
    cdn_urls = upload_assets(rel_id)
    update_paper(cdn_urls)

if __name__ == '__main__':
    main()
