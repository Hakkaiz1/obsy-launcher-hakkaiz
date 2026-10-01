import { useLauncherStore } from "@/state";
import { SettingsDialog } from "./SettingsDialog";
import { ConsoleDialog } from "./ConsoleDialog";
import { Button } from "@/components/ui/button";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Globe, MessageCircle } from "lucide-react";

export const Header = () => {
  const { state } = useLauncherStore();

  return (
    <header
      data-tauri-drag-region
      className="absolute top-0 right-0 z-20 flex cursor-default items-center justify-end p-4 select-none"
    >
      {state && (
        <div className="border-border/50 bg-card/50 flex items-center gap-2 rounded-xl border p-2 shadow-lg backdrop-blur-md">
          <ConsoleDialog />
          <SettingsDialog />
          <Button
            variant="ghost"
            size="sm"
            className="text-muted-foreground hover:text-foreground"
            onClick={() => void openUrl("https://dbcsuper.lojasquare.com.br/")}
          >
            <Globe />
            Site
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="text-muted-foreground hover:text-foreground"
            onClick={() => void openUrl("https://discord.gg/p3fTQ3KFbV")}
          >
            <MessageCircle />
            Discord
          </Button>
        </div>
      )}
    </header>
  );
};
