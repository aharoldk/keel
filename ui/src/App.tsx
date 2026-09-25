import { useEffect } from "react";
import { useKeel } from "@/state/store";
import { handleGlobalKeydown } from "@/commands";
import { ToastHost } from "@/components/ui";
import { Toolbar } from "@/features/shell/Toolbar";
import { Sidebar } from "@/features/shell/Sidebar";
import { StatusBar } from "@/features/shell/StatusBar";
import { ConsolePanel } from "@/features/console/ConsolePanel";
import { Welcome } from "@/features/onboarding/Welcome";
import { SettingsModal } from "@/features/settings/SettingsModal";
import { CommandPalette } from "@/features/palette/CommandPalette";
import { RequestTabsBar } from "@/features/request/RequestTabsBar";
import { RequestEditor } from "@/features/request/RequestEditor";
import { ResponseViewer } from "@/features/response/ResponseViewer";
import { FlowRun } from "@/features/flow/FlowPanel";
import { EnvironmentEditor } from "@/features/environments/EnvPanel";
import { RunnerModal } from "@/features/runner/RunnerModal";
import { CodegenModal } from "@/features/codegen/CodegenModal";
import { AiPanel } from "@/features/ai/AiPanel";

export default function App() {
  const ready = useKeel((s) => s.ready);
  const workspace = useKeel((s) => s.workspace);
  const activePath = useKeel((s) => s.activePath);
  const contentPanel = useKeel((s) => s.contentPanel);
  const consoleOpen = useKeel((s) => s.consoleOpen);
  const aiOpen = useKeel((s) => s.aiOpen);

  useEffect(() => {
    useKeel.getState().init();
  }, []);

  useEffect(() => {
    window.addEventListener("keydown", handleGlobalKeydown);
    return () => window.removeEventListener("keydown", handleGlobalKeydown);
  }, []);

  if (!ready) {
    return (
      <div className="h-full flex items-center justify-center bg-bg-0">
        <span className="animate-pulse text-fg-2 text-sm font-mono tracking-widest">
          KEEL
        </span>
      </div>
    );
  }

  if (!workspace) {
    return (
      <>
        <Welcome />
        <ToastHost />
      </>
    );
  }

  return (
    <div className="h-full flex flex-col bg-bg-0 text-fg-0">
      <Toolbar />
      <div className="flex-1 flex min-h-0">
        <Sidebar />
        <main className="flex-1 flex flex-col min-w-0 min-h-0 bg-bg-1 border-l border-line-0">
          <RequestTabsBar />
          {contentPanel?.kind === "flow" ? (
            <FlowRun />
          ) : contentPanel?.kind === "environment" ? (
            <EnvironmentEditor fileName={contentPanel.fileName} />
          ) : activePath ? (
            <RequestEditor />
          ) : (
            <div className="flex-1 flex items-center justify-center text-fg-2 text-xs">
              Select a request to begin
            </div>
          )}
        </main>
        {aiOpen && <AiPanel />}
      </div>
      {consoleOpen && <ConsolePanel />}
      <StatusBar />
      <SettingsModal />
      <CommandPalette />
      <RunnerModal />
      <CodegenModal />
      <ToastHost />
    </div>
  );
}
