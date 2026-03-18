#!/usr/bin/env python3
"""
Generate comparison plots for all simulation scenarios.

Creates visualizations showing:
- Velocity over time
- Default rates
- Inequality (Gini coefficient)
- Hoarding index
- Balance distributions
"""

import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
from pathlib import Path

# Set style
sns.set_style("whitegrid")
plt.rcParams['figure.figsize'] = (14, 10)

# Load all scenario results
scenarios = {
    'Baseline': 'results/baseline/metrics.csv',
    'Dynamic Limits': 'results/dynamic_limits/metrics.csv',
    'High Demurrage': 'results/high_demurrage/metrics.csv',
    'High Free Riders': 'results/high_free_riders/metrics.csv',
    'Low Trust': 'results/low_trust/metrics.csv'
}

data = {}
for name, path in scenarios.items():
    if Path(path).exists():
        df = pd.read_csv(path, index_col=0)
        df['Scenario'] = name
        data[name] = df

if not data:
    print("Error: No scenario results found. Run simulations first.")
    exit(1)

# Create 2x3 subplot grid
fig, axes = plt.subplots(2, 3, figsize=(18, 10))
fig.suptitle('Mutual Credit Economic Simulation - Scenario Comparison', fontsize=16, y=0.995)

# Plot 1: Velocity over time
ax = axes[0, 0]
for name, df in data.items():
    ax.plot(df['Month'], df['Velocity'], label=name, linewidth=2)
ax.set_xlabel('Month')
ax.set_ylabel('Velocity (credits/month)')
ax.set_title('Economic Velocity Over Time')
ax.legend()
ax.grid(True, alpha=0.3)

# Plot 2: Default Rate over time
ax = axes[0, 1]
for name, df in data.items():
    ax.plot(df['Month'], df['DefaultRate'] * 100, label=name, linewidth=2)
ax.set_xlabel('Month')
ax.set_ylabel('Default Rate (%)')
ax.set_title('Default Rate Over Time')
ax.legend()
ax.grid(True, alpha=0.3)

# Plot 3: Gini Coefficient over time
ax = axes[0, 2]
for name, df in data.items():
    ax.plot(df['Month'], df['Gini'], label=name, linewidth=2)
ax.set_xlabel('Month')
ax.set_ylabel('Gini Coefficient')
ax.set_title('Inequality (Gini) Over Time')
ax.axhline(y=0.7, color='r', linestyle='--', alpha=0.5, label='Danger threshold')
ax.legend()
ax.grid(True, alpha=0.3)

# Plot 4: Hoarding Index over time
ax = axes[1, 0]
for name, df in data.items():
    ax.plot(df['Month'], df['HoardingIndex'] * 100, label=name, linewidth=2)
ax.set_xlabel('Month')
ax.set_ylabel('Hoarding Index (%)')
ax.set_title('Hoarding Index Over Time')
ax.axhline(y=80, color='r', linestyle='--', alpha=0.5, label='Danger threshold')
ax.legend()
ax.grid(True, alpha=0.3)

# Plot 5: Total Transactions over time
ax = axes[1, 1]
for name, df in data.items():
    ax.plot(df['Month'], df['Transactions'], label=name, linewidth=2)
ax.set_xlabel('Month')
ax.set_ylabel('Cumulative Transactions')
ax.set_title('Transaction Volume Over Time')
ax.legend()
ax.grid(True, alpha=0.3)

# Plot 6: Final metrics comparison (bar chart)
ax = axes[1, 2]
final_metrics = {}
for name, df in data.items():
    final_row = df.iloc[-1]
    final_metrics[name] = {
        'Velocity': final_row['Velocity'] / 1000,  # Scale to thousands
        'Default %': final_row['DefaultRate'] * 100,
        'Gini': final_row['Gini'] * 10,  # Scale for visibility
        'Hoarding %': final_row['HoardingIndex'] * 100
    }

final_df = pd.DataFrame(final_metrics).T
x = range(len(final_df.columns))
width = 0.15
colors = plt.cm.tab10(range(len(final_df)))

for i, (scenario, color) in enumerate(zip(final_df.index, colors)):
    offset = (i - len(final_df) / 2) * width
    ax.bar([xi + offset for xi in x], final_df.loc[scenario], width, label=scenario, color=color)

ax.set_ylabel('Metric Value')
ax.set_title('Final Metrics Comparison\n(Velocity/1000, Gini×10)')
ax.set_xticks(x)
ax.set_xticklabels(final_df.columns, rotation=45, ha='right')
ax.legend(loc='upper left', fontsize=8)
ax.grid(True, alpha=0.3, axis='y')

plt.tight_layout()
plt.savefig('scenario_comparison.png', dpi=150, bbox_inches='tight')
print("✅ Saved scenario_comparison.png")

# Create second figure for detailed analysis
fig2, axes2 = plt.subplots(2, 2, figsize=(14, 10))
fig2.suptitle('Economic Health Indicators', fontsize=16, y=0.995)

# Plot 1: Agents in default
ax = axes2[0, 0]
for name, df in data.items():
    ax.plot(df['Month'], df['AgentsInDefault'], label=name, linewidth=2)
ax.set_xlabel('Month')
ax.set_ylabel('Number of Agents')
ax.set_title('Agents in Default Over Time')
ax.legend()
ax.grid(True, alpha=0.3)

# Plot 2: Balance distribution (positive vs negative)
ax = axes2[0, 1]
for name, df in data.items():
    final_row = df.iloc[-1]
    positive = final_row['PositiveBalances']
    negative = final_row['NegativeBalances']
    ax.bar(name, positive, label='Positive' if name == list(data.keys())[0] else '', color='green', alpha=0.7)
    ax.bar(name, -negative, label='Negative' if name == list(data.keys())[0] else '', color='red', alpha=0.7)
ax.set_ylabel('Number of Agents')
ax.set_title('Final Balance Distribution')
ax.set_xticklabels(list(data.keys()), rotation=45, ha='right')
ax.legend()
ax.axhline(y=0, color='black', linewidth=0.8)
ax.grid(True, alpha=0.3, axis='y')

# Plot 3: Average trust over time
ax = axes2[1, 0]
for name, df in data.items():
    ax.plot(df['Month'], df['AvgTrust'], label=name, linewidth=2)
ax.set_xlabel('Month')
ax.set_ylabel('Average Trust Score')
ax.set_title('Trust Evolution Over Time')
ax.axhline(y=0.5, color='gray', linestyle='--', alpha=0.5, label='Neutral trust')
ax.legend()
ax.grid(True, alpha=0.3)

# Plot 4: Average balance over time
ax = axes2[1, 1]
for name, df in data.items():
    ax.plot(df['Month'], df['AvgBalance'], label=name, linewidth=2)
ax.set_xlabel('Month')
ax.set_ylabel('Average Balance (credits)')
ax.set_title('Average Balance Over Time')
ax.axhline(y=0, color='black', linewidth=0.8, linestyle='--', alpha=0.5)
ax.legend()
ax.grid(True, alpha=0.3)

plt.tight_layout()
plt.savefig('health_indicators.png', dpi=150, bbox_inches='tight')
print("✅ Saved health_indicators.png")

print("\nPlots generated successfully!")
print("  - scenario_comparison.png: Main metrics comparison")
print("  - health_indicators.png: Economic health over time")
