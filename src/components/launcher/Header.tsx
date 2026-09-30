import { useLauncherStore } from "@/state";
import { SettingsDialog } from "./SettingsDialog";
import { ConsoleDialog } from "./ConsoleDialog";

export const Header = () => {
  const { state } = useLauncherStore();

  return (
    <header
      data-tauri-drag-region
      className="border-border/50 bg-card/50 flex w-full cursor-default items-center justify-between border-b p-4 backdrop-blur-md select-none"
    >
      {state && (
        <div className="flex items-center gap-2">
          <ConsoleDialog />
          <SettingsDialog />
        </div>
      )}
    </header>
  );
};
