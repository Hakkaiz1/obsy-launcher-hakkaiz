import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  LoaderCircle,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

interface InstalledPackManifest {
  pack_version: string;
  minecraft_version: string;
}

interface TechnicProgress {
  status: string;
  detail?: string | null;
}

interface TechnicPackStatusProps {
  error: string | null;
}

const statusTranslation: Record<string, string> = {
  loading_status: "technic.status.loading",
  checking_technic: "technic.status.checking",
  downloading_technic: "technic.status.downloading",
  applying_technic: "technic.status.applying",
  cached_offline: "technic.status.offline",
  ready: "technic.status.ready",
  not_installed: "technic.status.notInstalled",
};

export const TechnicPackStatus = ({ error }: TechnicPackStatusProps) => {
  const { t } = useTranslation();
  const [manifest, setManifest] = useState<InstalledPackManifest | null>(null);
  const [phase, setPhase] = useState("loading_status");
  const [detail, setDetail] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    let unlistenProgress: (() => void) | undefined;
    let unlistenWarning: (() => void) | undefined;

    const refreshStatus = async () => {
      try {
        const installed = await invoke<InstalledPackManifest | null>(
          "get_technic_pack_status",
        );
        if (mounted) {
          setManifest(installed);
          setStatusError(null);
          setPhase((current) =>
            current === "loading_status" ||
            current === "ready" ||
            current === "not_installed"
              ? installed
                ? "ready"
                : "not_installed"
              : current,
          );
        }
      } catch (statusError) {
        console.error("Failed to read DBC Super pack status:", statusError);
        if (mounted) setStatusError(String(statusError));
      }
    };

    void refreshStatus();
    void listen<TechnicProgress>("technic-progress", (event) => {
      const update = event.payload;
      setPhase(update.status);
      setDetail(update.detail ?? null);
      if (update.status === "checking_technic" || update.status === "ready") {
        setWarning(null);
      }
      if (update.status === "ready" || update.status === "cached_offline") {
        void refreshStatus();
      }
    }).then((unlisten) => {
      if (mounted) unlistenProgress = unlisten;
      else unlisten();
    });
    void listen<string>("technic-offline-warning", (event) => {
      setWarning(event.payload);
    }).then((unlisten) => {
      if (mounted) unlistenWarning = unlisten;
      else unlisten();
    });

    return () => {
      mounted = false;
      unlistenProgress?.();
      unlistenWarning?.();
    };
  }, []);

  const translatedStatus = t(
    statusTranslation[phase] ?? "technic.status.checking",
  );
  const isBusy =
    phase === "loading_status" ||
    phase === "checking_technic" ||
    phase === "downloading_technic" ||
    phase === "applying_technic";

  return (
    <section
      className="border-border/60 bg-background/45 relative z-10 flex flex-col gap-3 rounded-lg border p-4"
      aria-live="polite"
      aria-label={t("technic.title")}
    >
      <div className="flex items-center justify-between gap-3">
        <div>
          <h2 className="text-sm font-semibold">{t("technic.title")}</h2>
          <p className="text-muted-foreground text-xs">{t("technic.source")}</p>
        </div>
        {isBusy ? (
          <LoaderCircle className="text-primary h-4 w-4 animate-spin" />
        ) : error || statusError ? (
          <AlertTriangle className="text-destructive h-4 w-4" />
        ) : manifest ? (
          <CheckCircle2 className="h-4 w-4 text-emerald-500" />
        ) : (
          <Download className="text-muted-foreground h-4 w-4" />
        )}
      </div>

      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-muted-foreground">{translatedStatus}</span>
        {manifest && (
          <span className="font-medium">
            {t("technic.version", { version: manifest.pack_version })}
          </span>
        )}
      </div>

      {manifest && (
        <p className="text-muted-foreground text-xs">
          {t("technic.minecraftVersion", {
            version: manifest.minecraft_version,
          })}
        </p>
      )}

      {detail && isBusy && (
        <p className="text-muted-foreground text-xs">{detail}</p>
      )}

      {warning && (
        <p className="flex gap-2 text-xs text-amber-500">
          <AlertTriangle className="h-4 w-4 shrink-0" />
          <span>{warning}</span>
        </p>
      )}

      {(error || statusError) && (
        <p className="text-destructive text-xs">{error ?? statusError}</p>
      )}
    </section>
  );
};
