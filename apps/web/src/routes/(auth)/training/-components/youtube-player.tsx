import { extractYouTubeVideoId } from "@lindaflor/shared/lib/youtube-url";
import { UserPlus } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type YouTubePlayerProps = {
  youtubeUrl: string;
  onComplete: () => void;
  isEnrolled: boolean;
  isEnrolling: boolean;
  onEnroll: () => void;
};

declare global {
  interface Window {
    YT?: {
      Player: new (
        elementId: string,
        options: {
          videoId: string;
          playerVars?: Record<string, string | number | undefined>;
          events?: {
            onStateChange?: (event: { data: number }) => void;
            onReady?: () => void;
          };
        },
      ) => {
        destroy: () => void;
      };
      PlayerState: {
        ENDED: number;
      };
    };
    onYouTubeIframeAPIReady?: () => void;
  }
}

export function YouTubePlayer({
  youtubeUrl,
  onComplete,
  isEnrolled,
  isEnrolling,
  onEnroll,
}: YouTubePlayerProps) {
  const videoId = extractYouTubeVideoId(youtubeUrl);
  const containerId = `youtube-player-${useId().replaceAll(":", "-")}`;
  const [isReady, setIsReady] = useState(false);
  const onCompleteRef = useRef(onComplete);
  const videoIdRef = useRef(videoId);
  const mountedRef = useRef(true);

  useEffect(() => {
    onCompleteRef.current = onComplete;
    videoIdRef.current = videoId;
  });

  useEffect(() => {
    mountedRef.current = true;

    if (!videoId) {
      return () => {
        mountedRef.current = false;
      };
    }

    const existingScript = document.getElementById("youtube-iframe-api");
    if (!existingScript) {
      const tag = document.createElement("script");
      tag.id = "youtube-iframe-api";
      tag.src = "https://www.youtube.com/iframe_api";
      document.body.appendChild(tag);
    }

    let player: InstanceType<NonNullable<Window["YT"]>["Player"]> | undefined;

    const initPlayer = () => {
      if (!mountedRef.current || !window.YT?.Player || !videoIdRef.current) {
        return;
      }

      player = new window.YT.Player(containerId, {
        videoId: videoIdRef.current,
        playerVars: {
          enablejsapi: 1,
        },
        events: {
          onReady: () => {
            if (mountedRef.current) {
              setIsReady(true);
            }
          },
          onStateChange: (event) => {
            if (event.data === window.YT?.PlayerState.ENDED) {
              onCompleteRef.current();
            }
          },
        },
      });
    };

    const handleYouTubeIframeAPIReady = () => {
      initPlayer();
    };

    if (window.YT?.Player) {
      initPlayer();
    } else {
      window.onYouTubeIframeAPIReady = handleYouTubeIframeAPIReady;
    }

    return () => {
      mountedRef.current = false;

      if (window.onYouTubeIframeAPIReady === handleYouTubeIframeAPIReady) {
        window.onYouTubeIframeAPIReady = undefined;
      }

      setIsReady(false);
      player?.destroy();
    };
  }, [videoId, containerId]);

  if (!videoId) {
    return (
      <p className="text-sm text-destructive">
        URL do YouTube inválida. Atualize o vídeo no gerenciamento do curso.
      </p>
    );
  }

  return (
    <div className={cn("space-y-3")}>
      <div
        id={containerId}
        className={cn("aspect-video w-full rounded-t-lg bg-black")}
      />
      <div className="flex justify-between px-3">
        {!isEnrolled ? (
          <Button onClick={onEnroll} disabled={isEnrolling} variant="secondary">
            <UserPlus />
            {isEnrolling ? "Inscrevendo…" : "Inscrever-se"}
          </Button>
        ) : null}

        <Button
          onClick={onComplete}
          disabled={!isReady}
          variant="default"
          size="default"
        >
          Marcar como concluído
        </Button>
      </div>
    </div>
  );
}
