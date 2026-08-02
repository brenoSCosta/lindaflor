import { divIcon } from "leaflet";
import { ExternalLink, MapPin } from "lucide-react";
import { MapContainer, Marker, TileLayer } from "react-leaflet";

import { cn } from "@/lib/utils";

const markerIcon = divIcon({
  className: "",
  html: '<div style="width:18px;height:18px;border-radius:50%;background:var(--primary);border:3px solid #fff;box-shadow:0 0 0 2px rgba(0,0,0,0.2)"></div>',
  iconSize: [18, 18],
  iconAnchor: [9, 9],
});

type TankAssetMapProps = {
  latitude: number | null;
  longitude: number | null;
  tag: string;
  className?: string;
};

export function TankAssetMap({
  latitude,
  longitude,
  tag,
  className,
}: TankAssetMapProps) {
  const hasCoords = latitude != null && longitude != null;
  const position = hasCoords ? { lat: latitude, lng: longitude } : null;
  const googleMapsHref =
    position != null
      ? `https://www.google.com/maps/search/?api=1&query=${position.lat},${position.lng}`
      : null;

  return (
    <div
      className={cn(
        "overflow-hidden rounded-xl border bg-card shadow-sm",
        className,
      )}
    >
      <div className="relative">
        {position != null ? (
          <div className="h-48 w-full">
            <MapContainer
              key={`${position.lat},${position.lng}`}
              center={position}
              zoom={15}
              className="size-full"
              zoomControl={false}
              scrollWheelZoom={false}
              dragging={false}
              doubleClickZoom={false}
              attributionControl={false}
            >
              <TileLayer
                attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
                url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
                referrerPolicy="strict-origin-when-cross-origin"
              />
              <Marker position={position} icon={markerIcon} />
            </MapContainer>
          </div>
        ) : (
          <div className="flex h-48 flex-col items-center justify-center gap-2 bg-muted/40 px-4 text-center">
            <MapPin className="size-5 text-muted-foreground" aria-hidden />
            <p className="text-sm text-muted-foreground">
              Coordenadas não cadastradas para {tag}
            </p>
          </div>
        )}

        <div className="pointer-events-none absolute inset-x-0 bottom-0 bg-linear-to-t from-background/90 to-transparent px-4 pt-10 pb-3">
          <span className="inline-flex items-center gap-2 rounded-full border bg-background/90 px-3 py-1.5 text-[10px] font-bold tracking-widest text-muted-foreground uppercase backdrop-blur-sm">
            <MapPin className="size-3.5 text-primary" aria-hidden />
            Localização do ativo
          </span>
        </div>
      </div>

      {googleMapsHref != null ? (
        <a
          href={googleMapsHref}
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center justify-center gap-2 border-t px-4 py-2.5 text-xs font-semibold text-foreground transition-colors hover:bg-muted/50 hover:text-primary"
        >
          <ExternalLink className="size-3.5" aria-hidden />
          Ver no Google Maps
        </a>
      ) : null}
    </div>
  );
}
