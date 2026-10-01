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
      className="border-border/50 bg-card/50 flex w-full cursor-default items-center justify-end border-b p-4 backdrop-blur-md select-none"
    >
      {state && (
        <div className="flex items-center gap-2">
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
