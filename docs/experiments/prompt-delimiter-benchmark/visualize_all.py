import json
import os
import matplotlib.pyplot as plt
import numpy as np

def setup_academic_style():
    plt.rcParams.update({
        'font.family': 'sans-serif',
        'font.sans-serif': ['Arial', 'Helvetica', 'DejaVu Sans'],
        'font.size': 11,
        'axes.labelsize': 12,
        'axes.titlesize': 13,
        'xtick.labelsize': 10.5,
        'ytick.labelsize': 10.5,
        'legend.fontsize': 10,
        'figure.titlesize': 14,
        'figure.dpi': 300,
        'axes.edgecolor': '#333333',
        'axes.linewidth': 0.9,
        'grid.color': '#EAEAEA',
        'grid.linestyle': '--',
        'grid.linewidth': 0.6,
    })

def plot_baseline_vs_stress():
    if not (os.path.exists("data/summary_metrics.json") and os.path.exists("data/stress_summary_metrics.json")):
        return
    with open("data/summary_metrics.json", "r", encoding="utf-8") as f:
        base = json.load(f)
    with open("data/stress_summary_metrics.json", "r", encoding="utf-8") as f:
        stress = json.load(f)
        
    delims = ["xml", "markdown", "square_bracket", "plain_colon", "json"]
    labels = ["XML (< >)", "Markdown (###)", "Bracket ([ ])", "Colon (:)", "JSON ({ })"]
    
    base_scores = [base[d]["overall_mean_score"] for d in delims]
    stress_scores = [stress[d]["overall_mean_score"] for d in delims]
    
    x = np.arange(len(labels))
    width = 0.35
    
    fig, ax = plt.subplots(figsize=(6.8, 4.8))
    
    rects1 = ax.bar(x - width/2, base_scores, width, label='Baseline Tasks', color='#2E5B88', edgecolor='#222222', linewidth=0.8)
    rects2 = ax.bar(x + width/2, stress_scores, width, label='Stress / Adversarial Tasks', color='#D97A53', edgecolor='#222222', linewidth=0.8)
    
    ax.set_ylabel('Mean Score (%)', fontweight='bold')
    ax.set_title('Instruction Following: Baseline vs. Stress Robustness', pad=12, fontweight='bold')
    ax.set_xticks(x)
    ax.set_xticklabels(labels, rotation=15, fontweight='bold')
    ax.set_ylim(0, 118)
    ax.legend(frameon=True, facecolor='white', framealpha=0.9, loc='upper right')
    ax.grid(axis='y', alpha=0.7)
    
    for rects in [rects1, rects2]:
        for bar in rects:
            yval = bar.get_height()
            ax.text(bar.get_x() + bar.get_width()/2.0, yval + 1.5, f'{yval:.1f}%', ha='center', va='bottom', fontsize=9.5, fontweight='bold')
            
    plt.tight_layout()
    os.makedirs("figures", exist_ok=True)
    plt.savefig("figures/fig1_baseline_vs_stress.png", dpi=300)
    plt.close()
    print("[+] Saved figures/fig1_baseline_vs_stress.png")

def plot_stress_breakdown():
    if not os.path.exists("data/stress_summary_metrics.json"):
        return
    with open("data/stress_summary_metrics.json", "r", encoding="utf-8") as f:
        stress = json.load(f)
        
    delims = ["xml", "markdown", "square_bracket", "plain_colon", "json"]
    delim_labels = ["XML", "Markdown", "Bracket", "Colon", "JSON"]
    tasks = [
        "Stress T1 (Injection Escape)",
        "Stress T2 (Extreme Multi-Constraint)",
        "Stress T3 (Hierarchical Binding)",
        "Stress T4 (Code Symbol Collision)"
    ]
    task_short = ["T1: Delimiter Injection", "T2: Multi-Constraint", "T3: Deep Hierarchy", "T4: Symbol Collision"]
    
    x = np.arange(len(task_short))
    width = 0.15
    palette = ['#2E5B88', '#5A7D9A', '#D97A53', '#6A9A78', '#9B7FA8']
    
    fig, ax = plt.subplots(figsize=(7.4, 5.0))
    for i, (delim, label, color) in enumerate(zip(delims, delim_labels, palette)):
        scores = [stress[delim]["task_scores"][t] for t in tasks]
        offset = (i - 2) * width
        rects = ax.bar(x + offset, scores, width, label=label, color=color, edgecolor='#333333', linewidth=0.7)
        
    ax.set_ylabel('Stress Task Score (%)', fontweight='bold')
    ax.set_title('Brittleness Breakdown across Adversarial & Complex Tasks', pad=12, fontweight='bold')
    ax.set_xticks(x)
    ax.set_xticklabels(task_short, fontweight='bold')
    ax.set_ylim(0, 125)
    ax.legend(frameon=True, facecolor='white', framealpha=0.9, loc='upper right', ncol=2)
    ax.grid(axis='y', alpha=0.7)
    
    plt.tight_layout()
    plt.savefig("figures/fig2_stress_breakdown.png", dpi=300)
    plt.close()
    print("[+] Saved figures/fig2_stress_breakdown.png")

def plot_position_accuracy():
    if not os.path.exists("data/position_summary_metrics.json"):
        return
    with open("data/position_summary_metrics.json", "r", encoding="utf-8") as f:
        pos_data = json.load(f)
        
    delims = ["xml", "markdown", "square_bracket", "plain_colon", "json"]
    delim_labels = ["XML (< >)", "Markdown (###)", "Bracket ([ ])", "Colon (:)", "JSON ({ })"]
    positions = ["head", "middle", "tail"]
    pos_labels = ["Head (10%)", "Middle (50%)", "Tail (90%)"]
    
    fig, ax = plt.subplots(figsize=(7.0, 4.8))
    markers = ['o', 's', '^', 'D', 'v']
    palette = ['#2E5B88', '#5A7D9A', '#D97A53', '#6A9A78', '#9B7FA8']
    
    for delim, label, marker, color in zip(delims, delim_labels, markers, palette):
        scores = [pos_data[p][delim] for p in positions]
        ax.plot(pos_labels, scores, marker=marker, linewidth=2.0, markersize=8, label=label, color=color)
        
    ax.set_ylabel('Needle Retrieval & Execution Score (%)', fontweight='bold')
    ax.set_xlabel('Needle Position in Context Haystack', fontweight='bold', labelpad=8)
    ax.set_title('Positional Sensitivity & "Lost-in-the-Middle" by Delimiter', pad=12, fontweight='bold')
    ax.set_ylim(0, 115)
    ax.legend(frameon=True, facecolor='white', framealpha=0.9, loc='lower right')
    ax.grid(True, alpha=0.7)
    
    plt.tight_layout()
    plt.savefig("figures/fig3_position_sensitivity.png", dpi=300)
    plt.close()
    print("[+] Saved figures/fig3_position_sensitivity.png")

def main():
    setup_academic_style()
    plot_baseline_vs_stress()
    plot_stress_breakdown()
    plot_position_accuracy()

if __name__ == "__main__":
    main()
