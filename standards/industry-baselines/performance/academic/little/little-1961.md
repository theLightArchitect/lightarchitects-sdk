<!-- uuid: 335d2e82-58f0-4d4a-83d5-eadbdd33e8f6 -->
<!-- citation: Little, Operations Research 1961 | type: academic-foundation | re-pull: never -->
<!-- gate: [P], [O] -->

# Little's Law (1961)

**Citation**: J. D. C. Little, "A proof for the queuing formula: L = λW," *Operations Research*, vol. 9, no. 3, pp. 383–387, May–June 1961.

## Verbatim quote (load-bearing)

> "the average number of customers in a queueing system is equal to the average arrival rate times the average time spent in the system."

## Formula

```
L = λ · W
```

Where:
- `L` = average number of items in the system (work-in-progress, WIP)
- `λ` = average arrival rate (items per unit time, throughput)
- `W` = average time an item spends in the system (cycle time, lead time)

## Why this is universally applicable

Little's Law makes no assumptions about:
- Arrival process distribution (any distribution)
- Service time distribution (any distribution)
- Service discipline (FIFO, LIFO, priority, etc.)
- Number of servers

It holds as a **conservation law** for any stable queueing system — including software development workflows treated as queues.

## Why LASDLC LDB v1.0 cites this (D8d, D8h)

**D8d (Agent utilization / Flow Efficiency)**:
- Flow Efficiency = active_agent_time / total_agent_time_alive
- Composes with Little's Law via WIP measurement: if too many work items in the system (L high) without proportional throughput (λ stagnant), individual cycle time (W) balloons — agents look busy but throughput is poor

**D8h (Wave-boundary throughput)**:
- L = λW directly applies: avg waves/hour per active agent (λ) × avg time-in-wave (W) = WIP at wave boundaries (L)
- Phase 3 auto-dispatch readiness signal: if L grows but λ doesn't, system is taking on more work without delivering it

## Practical application

For an agentic build:
- Measure λ (e.g., merged PRs per week, gates passed per phase) via AYIN
- Measure W (lead time per work item, cycle time per phase)
- Compute L = λW; verify against observed WIP
- If observed WIP ≠ λW, there's measurement error or system instability — investigate

## Status

- **Type**: foundational paper; not updated
- **Re-pull cadence**: never; cite original publication verbatim
- **Used by**: LDB §7.7 D8d (Flow Efficiency) + D8h (wave-boundary throughput)
