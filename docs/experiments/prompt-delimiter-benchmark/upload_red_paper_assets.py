import os
import requests
import re

TOKEN = os.environ.get("GHCR_TOKEN")
REPO = "agnusdei1207/pentesting"
TAG_NAME = "red-paper-assets-v1"

HEADERS_API = {
    "Authorization": f"Bearer {TOKEN}",
    "Accept": "application/vnd.github+json"
}

# 1. 릴리즈 생성 또는 조회
def get_or_create_release():
    r_get = requests.get(f"https://api.github.com/repos/{REPO}/releases/tags/{TAG_NAME}", headers=HEADERS_API)
    if r_get.status_code == 200:
        rel = r_get.json()
        print(f"[+] Found existing release: {rel['id']}")
        return rel['id']
        
    rel_payload = {
        'tag_name': TAG_NAME,
        'target_commitish': 'main',
        'name': 'Red Paper Assets v1',
        'body': 'Research paper visualization figures and assets for red-paper',
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

# 2. 이미지 업로드
def upload_all_images(release_id):
    figures_dir = r"C:\workspace\red-paper\assets\figures"
    cdn_mapping = {}
    
    for f in os.listdir(figures_dir):
        if not f.endswith(".png"):
            continue
        fp = os.path.join(figures_dir, f)
        url = f"https://uploads.github.com/repos/{REPO}/releases/{release_id}/assets?name={f}"
        
        with open(fp, "rb") as file_obj:
            data = file_obj.read()
            
        headers = {
            "Authorization": f"Bearer {TOKEN}",
            "Content-Type": "image/png"
        }
        resp = requests.post(url, headers=headers, data=data)
        if resp.status_code in [200, 201]:
            dl_url = resp.json()["browser_download_url"]
            print(f"[+] Uploaded {f} -> {dl_url}")
            cdn_mapping[f] = dl_url
        elif resp.status_code == 422: # Already exists
            dl_url = f"https://github.com/{REPO}/releases/download/{TAG_NAME}/{f}"
            print(f"[!] Existing asset {f} -> {dl_url}")
            cdn_mapping[f] = dl_url
        else:
            print(f"[-] Upload failed for {f}: {resp.status_code} {resp.text}")
            
    return cdn_mapping

# 3. 마크다운 파일들의 이미지 경로 치환
def update_markdown_files(cdn_mapping):
    target_files = [
        r"C:\workspace\red-paper\06_manuscript\PAPER_FULL.md",
        r"C:\workspace\red-paper\06_manuscript\PAPER_SHORT.md",
        r"C:\workspace\red-paper\03_experiments\README.md"
    ]
    
    for tf in target_files:
        if not os.path.exists(tf):
            continue
        with open(tf, "r", encoding="utf-8") as file_obj:
            content = file_obj.read()
            
        replaced_count = 0
        for filename, cdn_url in cdn_mapping.items():
            # Matches: ../assets/figures/fig-xxx.png or assets/figures/fig-xxx.png
            pattern = re.compile(rf'\((\.\./assets/figures/|assets/figures/|\.\./\.\./assets/figures/)?{re.escape(filename)}\)')
            if pattern.search(content):
                content = pattern.sub(f'({cdn_url})', content)
                replaced_count += 1
                
        with open(tf, "w", encoding="utf-8") as file_obj:
            file_obj.write(content)
            
        print(f"[+] Updated {tf} ({replaced_count} images mapped to CDN)")

def main():
    print("[*] Starting Red Paper Image CDN Migration...")
    rel_id = get_or_create_release()
    if not rel_id:
        return
    cdn_mapping = upload_all_images(rel_id)
    update_markdown_files(cdn_mapping)
    print("\n[★] All Red Paper images successfully hosted on CDN and markdown files updated!")

if __name__ == "__main__":
    main()
