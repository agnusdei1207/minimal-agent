import json
import os
import matplotlib.pyplot as plt
import numpy as np

def load_data():
    with open("data/summary_metrics.json", "r", encoding="utf-8") as f:
        summary = json.load(f)
    with open("data/raw_results.json", "r", encoding="utf-8") as f:
        raw = json.load(f)
    return summary, raw

def setup_academic_style():
    plt.rcParams.update({
        'font.family': 'sans-serif',
        'font.sans-serif': ['Arial', 'Helvetica', 'DejaVu Sans'],
        'font.size': 11,
        'axes.labelsize': 12,
        'axes.titlesize': 13,
        'xtick.labelsize': 10.5,
        'ytick.labelsize': 10.5,
        'legend.fontsize': 10.5,
        'figure.titlesize': 14,
        'figure.dpi': 300,
        'axes.edgecolor': '#333333',
        'axes.linewidth': 0.9,
        'grid.color': '#E0E0E0',
        'grid.linestyle': '--',
        'grid.linewidth': 0.6,
    })

def plot_overall_performance(summary, raw):
    fig, ax = plt.subplots(figsize=(6.4, 4.8)) # 4:3 standard
    
    delims = ["xml", "markdown", "square_bracket", "plain_colon", "json"]
    labels = ["XML (< >)", "Markdown (###)", "Bracket ([ ])", "Colon (:)", "JSON ({ })"]
    
    means = [summary[d]["overall_mean_score"] for d in delims]
    
    # Calculate standard deviation from raw
    stds = []
    for d in delims:
        scores = [r["score"] for r in raw if r["delimiter"] == d]
        stds.append(np.std(scores))
        
    colors = ['#1f77b4', '#aec7e8', '#ff7f0e', '#2ca02c', '#9467bd']
    # Academic muted palette
    palette = ['#2E5B88', '#5A7D9A', '#D97A53', '#6A9A78', '#9B7FA8']
    
    bars = ax.bar(labels, means, yerr=stds, capsize=4, color=palette, width=0.55, edgecolor='#222222', linewidth=0.8)
    
    ax.set_ylabel('Mean Score (%)', fontweight='bold')
    ax.set_ylim(0, 115)
    ax.set_title('Overall Instruction Following Score by Delimiter', pad=12, fontweight='bold')
    ax.grid(axis='y', alpha=0.7)
    
    # Value labels
    for bar, mean, std in zip(bars, means, stds):
        yval = bar.get_height()
        ax.text(bar.get_x() + bar.get_width()/2.0, yval + std + 3.0, f'{mean:.1f}%', ha='center', va='bottom', fontsize=10, fontweight='bold')
        
    plt.xticks(rotation=15)
    plt.tight_layout()
    os.makedirs("figures", exist_ok=True)
    plt.savefig("figures/fig1_overall_performance.png", dpi=300)
    plt.close()
    print("[+] Saved figures/fig1_overall_performance.png")

def plot_task_breakdown(summary, raw):
    fig, ax = plt.subplots(figsize=(7.2, 5.0))
    
    delims = ["xml", "markdown", "square_bracket", "plain_colon", "json"]
    delim_labels = ["XML", "Markdown", "Bracket", "Colon", "JSON"]
    tasks = [
        "Task 1 (Multi-Constraint)",
        "Task 2 (Context Isolation)",
        "Task 3 (Schema Extraction)",
        "Task 4 (Boundary Isolation)"
    ]
    task_short = ["T1: Constraints", "T2: Isolation", "T3: Extraction", "T4: Boundary"]
    
    x = np.arange(len(task_short))
    width = 0.15
    palette = ['#2E5B88', '#5A7D9A', '#D97A53', '#6A9A78', '#9B7FA8']
    
    for i, (delim, label, color) in enumerate(zip(delims, delim_labels, palette)):
        scores = [summary[delim]["task_scores"][t] for t in tasks]
        offset = (i - 2) * width
        rects = ax.bar(x + offset, scores, width, label=label, color=color, edgecolor='#333333', linewidth=0.7)
        
    ax.set_ylabel('Task Score (%)', fontweight='bold')
    ax.set_title('Task-Specific Performance by Prompt Delimiter', pad=12, fontweight='bold')
    ax.set_xticks(x)
    ax.set_xticklabels(task_short, fontweight='bold')
    ax.set_ylim(0, 120)
    ax.legend(frameon=True, facecolor='white', framealpha=0.9, loc='upper right', ncol=2)
    ax.grid(axis='y', alpha=0.7)
    
    plt.tight_layout()
    plt.savefig("figures/fig2_task_breakdown.png", dpi=300)
    plt.close()
    print("[+] Saved figures/fig2_task_breakdown.png")

def main():
    setup_academic_style()
    if not os.path.exists("data/summary_metrics.json"):
        print("[-] data/summary_metrics.json not found yet.")
        return
    summary, raw = load_data()
    plot_overall_performance(summary, raw)
    plot_task_breakdown(summary, raw)

if __name__ == "__main__":
    main()
