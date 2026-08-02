import { useQuery } from "@tanstack/react-query";

import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { orpc } from "@/lib/orpc";

const TWENTY_THREE_HOURS_IN_MS = 23 * 60 * 60 * 1000;

interface UserAvatarProps {
  userId: string;
  image: string | null | undefined;
  name: string;
  fallback?: string;
  size?: "default" | "sm" | "lg";
  className?: string;
}

function isExternalUrl(value: string): boolean {
  return value.includes("://");
}

export function UserAvatar({
  userId,
  image,
  name,
  fallback,
  size = "default",
  className,
}: UserAvatarProps) {
  const initial = (fallback ?? name?.[0] ?? "?").toUpperCase();

  const needsSignedUrl = Boolean(image && !isExternalUrl(image));

  const { data } = useQuery(
    orpc.user.v1.avatar.get.queryOptions({
      input: { id: userId },
      staleTime: TWENTY_THREE_HOURS_IN_MS,
      refetchInterval: TWENTY_THREE_HOURS_IN_MS,
      enabled: needsSignedUrl,
    }),
  );

  const src = (() => {
    if (!image) {
      return null;
    }

    if (isExternalUrl(image)) {
      return image;
    }

    return data?.url ?? null;
  })();

  return (
    <Avatar size={size} className={className}>
      {src ? <AvatarImage src={src} alt={name} /> : undefined}
      <AvatarFallback>{initial}</AvatarFallback>
    </Avatar>
  );
}
