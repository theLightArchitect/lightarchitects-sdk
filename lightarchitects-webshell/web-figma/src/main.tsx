
  import { createRoot } from "react-dom/client";
  import App from "./app/App.tsx";
  import "./styles/index.css";
  import { EngineeringProvider } from "./engineering/store/EngineeringProvider";
  import { StatusBar } from "./engineering/chrome/StatusBar";
  import { ScopeRail } from "./engineering/scope/ScopeRail";
  import { CommandPalette } from "./engineering/palette/CommandPalette";

  createRoot(document.getElementById("root")!).render(
    <EngineeringProvider>
      <App />
      <StatusBar />
      <ScopeRail />
      <CommandPalette />
    </EngineeringProvider>
  );
