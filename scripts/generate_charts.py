import json
import os
import matplotlib.pyplot as plt
import numpy as np

# Load data
kpi_path = r'C:\workspace\minimal-agent\benchmarks\zai\glm-5.3-flash\artifacts\reports\kpi.json'
with open(kpi_path, 'r', encoding='utf-8') as f:
    kpi = json.load(f)

tasks = kpi.get('tasks', [])
solved_tasks = [t for t in tasks if t.get('outcome') == 'solved']
failed_tasks = [t for t in tasks if t.get('outcome') != 'solved']

out_dir = r'C:\workspace\minimal-agent\assets'
os.makedirs(out_dir, exist_ok=True)

# Common style
plt.style.use('dark_background')
colors_solved = '#4CAF50'
colors_failed = '#F44336'
font_color = '#EEEEEE'

def setup_plot(figsize=(8, 5)):
    fig, ax = plt.subplots(figsize=figsize)
    fig.patch.set_facecolor('#1E1E1E')
    ax.set_facecolor('#1E1E1E')
    ax.spines['top'].set_visible(False)
    ax.spines['right'].set_visible(False)
    ax.spines['left'].set_color('#444444')
    ax.spines['bottom'].set_color('#444444')
    ax.tick_params(colors=font_color)
    return fig, ax

# 1. Outcome Mix (Donut Chart)
fig, ax = plt.subplots(figsize=(6, 6))
fig.patch.set_facecolor('#1E1E1E')
sizes = [len(solved_tasks), len(failed_tasks)]
labels = [f'Solved\n({sizes[0]})', f'Unsolved\n({sizes[1]})']
colors = [colors_solved, colors_failed]
wedges, texts, autotexts = ax.pie(sizes, labels=labels, colors=colors, autopct='%1.1f%%', 
                                  startangle=90, pctdistance=0.85, 
                                  textprops={'color': font_color, 'fontsize': 14, 'weight': 'bold'})
# Draw circle for donut
centre_circle = plt.Circle((0,0),0.70,fc='#1E1E1E')
fig.gca().add_artist(centre_circle)
ax.axis('equal')  
plt.tight_layout()
plt.savefig(os.path.join(out_dir, 'outcome_mix.png'), dpi=150, bbox_inches='tight', facecolor=fig.get_facecolor())
plt.close()

# 2. Solve Rate by Level
levels = {}
for t in tasks:
    lvl = t.get('level', 'Unknown')
    if lvl not in levels:
        levels[lvl] = {'total': 0, 'solved': 0}
    levels[lvl]['total'] += 1
    if t.get('outcome') == 'solved':
        levels[lvl]['solved'] += 1

sorted_levels = sorted([l for l in levels.keys() if l != 'Unknown'], key=lambda x: int(x) if str(x).isdigit() else str(x))
lvls = [f"Level {l}" for l in sorted_levels]
totals = [levels[l]['total'] for l in sorted_levels]
solved = [levels[l]['solved'] for l in sorted_levels]

fig, ax = setup_plot((8, 5))
x = np.arange(len(lvls))
width = 0.6
ax.bar(x, totals, width, label='Total', color='#333333')
ax.bar(x, solved, width, label='Solved', color=colors_solved)
ax.set_xticks(x)
ax.set_xticklabels(lvls, fontsize=12)
for i, (t, s) in enumerate(zip(totals, solved)):
    ax.text(i, s + 0.5, f"{s}/{t}", ha='center', color=font_color, fontsize=12, fontweight='bold')
plt.tight_layout()
plt.savefig(os.path.join(out_dir, 'solve_rate_by_level.png'), dpi=150, bbox_inches='tight', facecolor=fig.get_facecolor())
plt.close()

# 3. Solve Rate by Tag (Top 10)
tags = {}
for t in tasks:
    for tag in t.get('tags', []):
        if tag not in tags:
            tags[tag] = {'total': 0, 'solved': 0}
        tags[tag]['total'] += 1
        if t.get('outcome') == 'solved':
            tags[tag]['solved'] += 1

# Sort by total tasks descending, take top 10
sorted_tags = sorted(tags.items(), key=lambda item: item[1]['total'], reverse=True)[:10]
sorted_tags = sorted_tags[::-1] # reverse for horizontal bar chart

tag_names = [item[0].replace('_', ' ').title() for item in sorted_tags]
tag_totals = [item[1]['total'] for item in sorted_tags]
tag_solved = [item[1]['solved'] for item in sorted_tags]

fig, ax = setup_plot((8, 6))
y = np.arange(len(tag_names))
ax.barh(y, tag_totals, label='Total', color='#333333')
ax.barh(y, tag_solved, label='Solved', color=colors_solved)
ax.set_yticks(y)
ax.set_yticklabels(tag_names, fontsize=12)
for i, (t, s) in enumerate(zip(tag_totals, tag_solved)):
    ax.text(s + 0.3, i, f"{s}/{t}", va='center', color=font_color, fontsize=11, fontweight='bold')
plt.tight_layout()
plt.savefig(os.path.join(out_dir, 'solve_rate_by_tag.png'), dpi=150, bbox_inches='tight', facecolor=fig.get_facecolor())
plt.close()

# 4. Token Usage (Histogram)
tokens = [t.get('usage', {}).get('total_tokens', 0)/1e6 for t in tasks if t.get('usage', {}).get('total_tokens', 0) > 0]
fig, ax = setup_plot((8, 5))
n, bins, patches = ax.hist(tokens, bins=20, color='#2196F3', edgecolor='#1E1E1E')
ax.set_xlabel('Total Tokens (Millions)', fontsize=12, color=font_color)
ax.set_ylabel('Tasks', fontsize=12, color=font_color)
plt.tight_layout()
plt.savefig(os.path.join(out_dir, 'token_usage.png'), dpi=150, bbox_inches='tight', facecolor=fig.get_facecolor())
plt.close()

print("Charts generated in assets/")
