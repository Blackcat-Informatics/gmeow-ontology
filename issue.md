# Issue #2850: Amend CONSTITUTION.md 221-222 to agree with standpoint decision D3

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: documentation

## Body

Amend CONSTITUTION.md lines 221-222 so the standpoint suppression rule agrees with decision D3

Lines 221-222 say a withdrawn standpoint or closed gmeow:StandpointTenure is suppressed, not deleted. Decision D3 separates a tenure whose holder withdrew it (suppressed) from one that merely closed (not suppressed). The same lines cite tests/test_standpoint.py and tests/test_suppress_gen.py, which no longer exist.


## Comments (1)

### coderabbitai — 2026-11-16T15:30:29Z

<!-- coderabbit-bot-generated:v1 -->

<!-- This is an auto-generated issue plan by CodeRabbit -->
<details>
<summary>🔗 Related PRs</summary>
<dl>
<dd>
<br>

Blackcat-Informatics/gmeow-ontology#2843 - Set the display flag only for a holder's own withdrawal [open]
</dd>
</dl>
</details>

---
<details>
<summary>📝 Issue Planner</summary>
<dl>
<dd>
<br>

<sub>Check the box below or use the `@coderabbitai plan` command to generate an implementation plan and prompts that you can use with your favorite coding assistant.</sub>

- [ ] <!-- {"checkboxId":"8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan
</dd>
</dl>
</details>


---
<details>
<summary> 🧪 Issue enrichment is currently in open beta.</summary>
<dl>
<dd>
<br>


You can configure auto-planning by selecting labels in the issue_enrichment configuration.

To disable automatic issue enrichment, add the following to your `.coderabbit.yaml`:
```yaml
issue_enrichment:
  auto_enrich:
    enabled: false
```
</dd>
</dl>
</details>

💬 Have feedback or questions? Drop into our [discord](https://discord.gg/coderabbit)!

