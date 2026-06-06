# Screen Flow — Scope Navigation User Journey

```mermaid
flowchart TD
    START([Operator opens webshell]) --> LAND_D0

    LAND_D0["/#/cockpit/platform\nd0 CockpitPlatform\n6 bento cards\n(NorthstarPulse, StrandMosaic,\nSquadConstellation, SmartDispatch,\nBuildHealth, WorkerFleet)"]

    LAND_D0 -->|click ProjectCard| TRANS_D0_D1{{FLIP transition\n400ms scope-d1 accent\nclear selection}}
    TRANS_D0_D1 --> LAND_D1

    LAND_D1["/#/cockpit/project/:id\nd1 CockpitProject\n8 bento cards\n+ BottomBar (WaveComposer + SmartDispatch)"]

    LAND_D1 -->|click Build in BuildsRail| SEL_BUILD{{selection = build\nright drawer → BuildFocus\nNO navigation}}
    SEL_BUILD --> LAND_D1

    LAND_D1 -->|⌘Enter on selected build\nOR click drill icon| TRANS_D1_D2{{FLIP transition\n400ms scope-d2 accent\nclear selection}}
    TRANS_D1_D2 --> LAND_D2

    LAND_D1 -->|click Worker in Fleet| SEL_WORKER{{selection = worker\nright drawer → WorkerFocus\nNO navigation}}
    SEL_WORKER --> LAND_D1

    LAND_D1 -->|click HITL item| SEL_ESCALATION{{selection = escalation\nright drawer → EscalationFocus\na/r = approve/reject}}
    SEL_ESCALATION --> LAND_D1

    LAND_D2["/#/cockpit/build/:codename\nd2 CockpitBuild\n7 bento cards\n+ BottomBar (WaveComposer)"]

    LAND_D2 -->|click file in trace| SEL_SPAN{{selection = span\nright drawer → SpanFocus\nNO navigation}}
    SEL_SPAN --> LAND_D2

    LAND_D2 -->|click gate cell| SEL_GATE{{selection = gate\nright drawer → GateFocus}}
    SEL_GATE --> LAND_D2

    LAND_D2 -->|⌘Enter on file | TRANS_D2_D3{{FLIP transition\n400ms scope-d3 accent}}
    TRANS_D2_D3 --> LAND_D3

    LAND_D3["/#/cockpit/file/:codename/:path\nd3 CockpitFile\n1 hero panel (full center)\nno bottom bar"]

    LAND_D3 -->|⌘[ / browser back| LAND_D2
    LAND_D2 -->|⌘[ / browser back| LAND_D1
    LAND_D1 -->|⌘[ / browser back| LAND_D0

    LAND_D0 -->|⌘K command palette| PALETTE{{Quick-Pick Palette\nany target type}}
    LAND_D1 -->|⌘K command palette| PALETTE
    LAND_D2 -->|⌘K command palette| PALETTE
    PALETTE -->|select| TRANS_D0_D1
    PALETTE -->|select| TRANS_D1_D2
```
