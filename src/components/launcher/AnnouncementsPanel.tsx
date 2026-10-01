import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";

const REFRESH_INTERVAL_MS = 5 * 60 * 1000;

interface Announcement {
  id: string;
  content: string;
  author: string;
  timestamp: string;
  attachments: string[];
}

interface AnnouncementChannel {
  cursor: string;
  messages: Announcement[];
}

interface AnnouncementFeed {
  schemaVersion: number;
  generatedAt: string;
  updates: AnnouncementChannel;
  patchNotes: AnnouncementChannel;
}

interface AnnouncementsPanelProps {
  activeSection: "updates" | "patch-notes";
}

const formatTimestamp = (timestamp: string) => {
  const date = new Date(timestamp);
  if (Number.isNaN(date.valueOf())) return timestamp;
  return new Intl.DateTimeFormat("pt-BR", {
    dateStyle: "short",
    timeStyle: "short",
  }).format(date);
};

export const AnnouncementsPanel = ({
  activeSection,
}: AnnouncementsPanelProps) => {
  const [feed, setFeed] = useState<AnnouncementFeed | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refreshAnnouncements = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const nextFeed = await invoke<AnnouncementFeed>(
        "get_discord_announcements",
      );
      setFeed(nextFeed);
    } catch (reason) {
      console.error("Failed to load Discord announcements", reason);
      setError(`Não foi possível carregar as publicações: ${String(reason)}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshAnnouncements();
    const interval = window.setInterval(
      () => void refreshAnnouncements(),
      REFRESH_INTERVAL_MS,
    );
    return () => window.clearInterval(interval);
  }, [refreshAnnouncements]);

  const channel =
    activeSection === "updates" ? feed?.updates : feed?.patchNotes;

  const openAttachment = async (url: string) => {
    try {
      await openUrl(url);
    } catch (reason) {
      console.error("Failed to open announcement attachment", reason);
      setError(`Não foi possível abrir o anexo: ${String(reason)}`);
    }
  };

  if (loading && !feed) {
    return (
      <p className="text-muted-foreground text-sm" role="status">
        Carregando publicações...
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <span className="text-muted-foreground text-xs">
          {loading ? "Atualizando..." : ""}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => void refreshAnnouncements()}
          disabled={loading}
        >
          <RefreshCw className="h-4 w-4" />
          Atualizar
        </Button>
      </div>

      {error && (
        <div
          className="border-destructive/40 bg-destructive/10 text-destructive flex items-center justify-between gap-3 rounded-lg border p-3 text-sm"
          role="alert"
        >
          <span>{error}</span>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => void refreshAnnouncements()}
            disabled={loading}
          >
            Tentar novamente
          </Button>
        </div>
      )}

      {!error && !channel?.messages.length && (
        <p className="text-muted-foreground text-sm">
          Ainda não há publicações nesta seção.
        </p>
      )}

      {channel?.messages.map((message) => (
        <article
          key={message.id}
          className="border-border/50 bg-card/50 flex flex-col gap-2 rounded-lg border p-4"
        >
          <div className="text-muted-foreground flex flex-wrap items-center justify-between gap-2 text-xs">
            <span className="font-medium">{message.author}</span>
            <time dateTime={message.timestamp}>
              {formatTimestamp(message.timestamp)}
            </time>
          </div>
          <p className="text-foreground text-sm break-words whitespace-pre-wrap">
            {message.content}
          </p>
          {!!message.attachments.length && (
            <div className="flex flex-wrap gap-2">
              {message.attachments.map((url) => (
                <Button
                  key={url}
                  type="button"
                  variant="link"
                  size="sm"
                  onClick={() => void openAttachment(url)}
                >
                  <ExternalLink className="h-3.5 w-3.5" />
                  Ver anexo
                </Button>
              ))}
            </div>
          )}
        </article>
      ))}
    </div>
  );
};
