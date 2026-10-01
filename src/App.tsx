import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Download, FileText } from "lucide-react";
import { Header } from "./components/launcher/Header";
import { addGameLog } from "./lib/logger";
import { LaunchButton } from "./components/launcher/LaunchButton";
import { ProfileSelector } from "./components/launcher/ProfileSelector";
import { TechnicPackStatus } from "./components/launcher/TechnicPackStatus";
import "./i18n";
import { useLauncherStore } from "./state";
import {
  AnimatePresence,
  LazyMotion,
  MotionConfig,
  domAnimation,
  m,
} from "framer-motion";

const App = () => {
  const {
    state,
    fetchState,
    fetchProfiles,
    fetchStartupTime,
    fetchAppMemory,
    refreshProfileSkin,
    refreshProfileToken,
  } = useLauncherStore();
  const { t, i18n } = useTranslation();
  const [updateStatus, setUpdateStatus] = useState<string | null>(null);
  const [technicError, setTechnicError] = useState<string | null>(null);
  const [activeSection, setActiveSection] = useState<"updates" | "patch-notes">(
    "updates",
  );

  useEffect(() => {
    let isMounted = true;

    const checkForAppUpdates = async () => {
      try {
        const update = await check();
        if (update && isMounted) {
          setUpdateStatus(
            t("updater.downloading", { version: update.version }),
          );
          let downloaded = 0;
          let contentLength = 0;
          await update.downloadAndInstall((event) => {
            if (!isMounted) return;
            switch (event.event) {
              case "Started":
                contentLength = event.data.contentLength || 0;
                break;
              case "Progress":
                downloaded += event.data.chunkLength;
                if (contentLength > 0) {
                  setUpdateStatus(
                    t("updater.downloadingProgress", {
                      version: update.version,
                      progress: Math.round((downloaded / contentLength) * 100),
                    }),
                  );
                }
                break;
              case "Finished":
                setUpdateStatus(t("updater.restarting"));
                break;
            }
          });

          if (isMounted) {
            await relaunch();
          }
        }
      } catch (error) {
        console.error("Failed to check for updates", error);
      }
    };
    checkForAppUpdates();
    fetchStartupTime().then(() => {
      const ms = useLauncherStore.getState().startupTimeMs;
      if (ms) {
        addGameLog("info", `[Obsy] Launcher initialized in ${ms} ms`);
      }
    });
    fetchAppMemory();
    fetchState();
    fetchProfiles();

    const memInterval = setInterval(() => {
      fetchAppMemory();
    }, 5000);

    return () => {
      isMounted = false;
      clearInterval(memInterval);
    };
  }, [t, fetchState, fetchProfiles, fetchStartupTime, fetchAppMemory]);

  useEffect(() => {
    const unlistenLog = listen<string>("minecraft-log", (event) => {
      addGameLog("info", `[Minecraft] ${event.payload}`);
    });
    const unlistenErr = listen<string>("minecraft-error", (event) => {
      addGameLog("error", `[Minecraft] ${event.payload}`);
    });

    return () => {
      unlistenLog.then((f) => f());
      unlistenErr.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (state?.language) {
      i18n.changeLanguage("pt-BR");
    }
  }, [state?.language, i18n]);

  useEffect(() => {
    if (state?.selectedProfileId) {
      refreshProfileToken(state.selectedProfileId);
      refreshProfileSkin(state.selectedProfileId);
    }
  }, [state?.selectedProfileId, refreshProfileSkin, refreshProfileToken]);

  return (
    <LazyMotion features={domAnimation}>
      <MotionConfig reducedMotion="user">
        <div className="text-foreground flex h-screen w-screen flex-col overflow-hidden bg-transparent font-sans">
          <Header />
          <main className="relative z-10 flex min-h-0 flex-1 gap-4 p-2">
            <AnimatePresence>
              {state && (
                <m.div
                  initial={{ opacity: 0, y: 20, scale: 0.95 }}
                  animate={{ opacity: 1, y: 0, scale: 1 }}
                  transition={{ duration: 0.5, type: "spring", bounce: 0.3 }}
                  className="relative flex min-h-0 min-w-0 flex-1"
                >
                  <div className="bg-card/70 border-border/50 relative flex h-full w-[56%] max-w-[800px] overflow-hidden rounded-xl border shadow-2xl backdrop-blur-md">
                    <section className="relative flex min-w-0 flex-1 flex-col">
                      <header className="border-border/50 flex h-16 shrink-0 items-center justify-center border-b px-5">
                        <nav
                          aria-label="Navegação principal"
                          className="flex items-center justify-end gap-2"
                        >
                          <button
                            type="button"
                            aria-current={
                              activeSection === "updates" ? "page" : undefined
                            }
                            onClick={() => setActiveSection("updates")}
                            className={`flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors ${
                              activeSection === "updates"
                                ? "bg-primary text-primary-foreground"
                                : "text-muted-foreground hover:text-foreground hover:bg-white/10"
                            }`}
                          >
                            <Download className="h-4 w-4" />
                            Atualizações
                          </button>
                          <button
                            type="button"
                            aria-current={
                              activeSection === "patch-notes"
                                ? "page"
                                : undefined
                            }
                            onClick={() => setActiveSection("patch-notes")}
                            className={`flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors ${
                              activeSection === "patch-notes"
                                ? "bg-primary text-primary-foreground"
                                : "text-muted-foreground hover:text-foreground hover:bg-white/10"
                            }`}
                          >
                            <FileText className="h-4 w-4" />
                            Patch Notes
                          </button>
                        </nav>
                      </header>
                      <div className="relative flex-1 overflow-y-auto p-5">
                        {activeSection === "updates" && updateStatus && (
                          <m.p
                            initial={{ opacity: 0, y: -10 }}
                            animate={{ opacity: 1, y: 0 }}
                            className="text-primary text-sm font-medium"
                          >
                            {updateStatus}
                          </m.p>
                        )}
                        <p className="text-muted-foreground text-sm">
                          {activeSection === "updates"
                            ? "As atualizações e novidades do launcher aparecerão aqui."
                            : "As notas das atualizações do servidor aparecerão aqui."}
                        </p>
                      </div>
                    </section>
                  </div>

                  <aside className="bg-card/90 border-border/50 absolute right-0 bottom-0 z-20 flex max-h-[calc(100%-1rem)] w-80 max-w-[calc(100vw-1rem)] flex-col gap-4 overflow-y-auto rounded-xl border p-4 shadow-2xl backdrop-blur-md">
                    <div className="flex flex-col gap-4">
                      <ProfileSelector />
                      <TechnicPackStatus error={technicError} />
                      <LaunchButton
                        onLaunchStart={() => setTechnicError(null)}
                        onLaunchError={setTechnicError}
                      />
                    </div>
                  </aside>
                </m.div>
              )}
            </AnimatePresence>
          </main>
        </div>
      </MotionConfig>
    </LazyMotion>
  );
};

export default App;
