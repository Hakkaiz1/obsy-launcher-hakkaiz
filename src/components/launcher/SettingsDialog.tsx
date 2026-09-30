import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { useLauncherStore } from "@/state";
import { getVersion } from "@tauri-apps/api/app";
import { Settings, Clock } from "lucide-react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

export const SettingsDialog = () => {
  const { state, updateState } = useLauncherStore();
  const { t } = useTranslation();
  const [appVersion, setAppVersion] = useState<string>("0.1.7");
  const [totalPlaytime, setTotalPlaytime] = useState<string | null>(null);

  useEffect(() => {
    getVersion()
      .then((ver) => setAppVersion(ver))
      .catch(() => {});

    invoke<{ formattedTotal: string; totalSeconds: number }>(
      "get_playtime_summary",
    )
      .then((res) => {
        if (res && res.totalSeconds > 0) {
          setTotalPlaytime(res.formattedTotal);
        }
      })
      .catch(() => {});
  }, []);

  if (!state) return null;

  return (
    <Dialog>
      <DialogTrigger
        render={
          <Button
            variant="ghost"
            size="icon"
            className="text-muted-foreground hover:text-foreground"
          />
        }
      >
        <Settings className="h-5 w-5" />
      </DialogTrigger>
      <DialogContent data-settings-dialog="true" className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle>{t("settings.title")}</DialogTitle>
        </DialogHeader>
        <div
          data-settings-scroll
          className="flex min-h-0 flex-col gap-3 overflow-x-hidden overflow-y-auto py-1 pr-1"
        >
          <section className="border-border/50 bg-card/70 flex flex-col gap-3 rounded-lg border p-3">
            <h3 className="text-primary/80 text-xs font-semibold tracking-wide uppercase">
              {t("settings.game")}
            </h3>
            <div className="flex items-center justify-between">
              <Label
                htmlFor="auto-memory"
                className="flex flex-col items-start gap-1"
              >
                <span>{t("settings.autoMemory")}</span>
              </Label>
              <Switch
                id="auto-memory"
                checked={state.autoMemory}
                onCheckedChange={(checked) =>
                  updateState({ ...state, autoMemory: checked })
                }
              />
            </div>

            <div className="flex flex-col gap-2">
              <div className="flex justify-between">
                <Label>{t("settings.memory")}</Label>
                <span className="text-muted-foreground text-sm">
                  {state.memoryAmount} MB
                </span>
              </div>
              <Slider
                disabled={state.autoMemory}
                value={[state.memoryAmount]}
                max={16384}
                min={512}
                step={512}
                onValueChange={(val: number | readonly number[]) =>
                  updateState({
                    ...state,
                    memoryAmount: Array.isArray(val) ? val[0] : val,
                  })
                }
              />
            </div>

            <div className="flex flex-col gap-2">
              <div className="flex items-center justify-between">
                <Label>{t("settings.jvmArgs")}</Label>
                <button
                  type="button"
                  onClick={() =>
                    updateState({
                      ...state,
                      jvmArguments:
                        "-XX:+UseG1GC -XX:+ParallelRefProcEnabled -XX:MaxGCPauseMillis=200 -XX:+UnlockExperimentalVMOptions -XX:+DisableExplicitGC -XX:+AlwaysPreTouch -XX:G1NewSizePercent=30 -XX:G1MaxNewSizePercent=40 -XX:G1ReservePercent=20 -XX:G1HeapRegionSize=8M",
                    })
                  }
                  className="text-primary cursor-pointer text-xs hover:underline"
                >
                  {t("settings.applyAikar")}
                </button>
              </div>
              <Input
                value={state.jvmArguments}
                onChange={(e) =>
                  updateState({
                    ...state,
                    jvmArguments: e.target.value,
                  })
                }
                placeholder="-XX:+UseG1GC -XX:+ParallelRefProcEnabled ..."
              />
              <p className="text-muted-foreground text-[11px]">
                {t("settings.aikarHint")}
              </p>
            </div>

            <div className="flex flex-col gap-2">
              <Label>{t("settings.javaPath") || "Caminho do Java"}</Label>
              <Input
                value={state.javaPath || ""}
                onChange={(e) =>
                  updateState({
                    ...state,
                    javaPath: e.target.value || null,
                  })
                }
                placeholder="Caminho para o Java (deixe vazio para usar o padrão)"
              />
            </div>

            <div className="flex items-center justify-between">
              <Label
                htmlFor="close-after-launch"
                className="flex flex-col items-start gap-1"
              >
                <span>{t("settings.closeAfterLaunch")}</span>
              </Label>
              <Switch
                id="close-after-launch"
                checked={state.closeAfterLaunch}
                onCheckedChange={(checked) =>
                  updateState({ ...state, closeAfterLaunch: checked })
                }
              />
            </div>
          </section>

          <section className="border-border/50 bg-card/70 flex flex-col gap-3 rounded-lg border p-3">
            <h3 className="text-primary/80 text-xs font-semibold tracking-wide uppercase">
              {t("settings.window")}
            </h3>
            <div className="grid grid-cols-2 gap-3">
              <div className="flex flex-col gap-1.5">
                <Label>{t("settings.width")}</Label>
                <Input
                  type="number"
                  value={state.screenWidth}
                  onChange={(e) =>
                    updateState({
                      ...state,
                      screenWidth: parseInt(e.target.value) || 854,
                    })
                  }
                />
              </div>
              <div className="flex flex-col gap-1.5">
                <Label>{t("settings.height")}</Label>
                <Input
                  type="number"
                  value={state.screenHeight}
                  onChange={(e) =>
                    updateState({
                      ...state,
                      screenHeight: parseInt(e.target.value) || 480,
                    })
                  }
                />
              </div>
            </div>

            <div className="flex items-center justify-between">
              <Label
                htmlFor="fullscreen"
                className="flex flex-col items-start gap-1"
              >
                <span>{t("settings.fullscreen")}</span>
              </Label>
              <Switch
                id="fullscreen"
                checked={state.fullscreen}
                onCheckedChange={(checked) =>
                  updateState({ ...state, fullscreen: checked })
                }
              />
            </div>
          </section>
        </div>

        <div className="border-border/40 text-muted-foreground flex items-center justify-between border-t pt-3 font-mono text-[11px]">
          <span>DBC Super Launcher v{appVersion}</span>
          <div className="flex items-center gap-3">
            {totalPlaytime && (
              <span className="flex items-center gap-1 text-amber-400">
                <Clock className="h-3 w-3" />
                {t("playtime.title")}: {totalPlaytime}
              </span>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
};
