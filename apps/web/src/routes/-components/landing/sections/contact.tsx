import { divIcon } from "leaflet";
import { ArrowUpRight, Mail, MapPin } from "lucide-react";
import { MapContainer, Marker, TileLayer } from "react-leaflet";

import { buttonVariants } from "@/components/ui/button";
import { Card, CardFooter } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { contact, offices } from "@/routes/-components/landing/content";
import { WhatsappIcon } from "@/routes/-components/landing/icons/whatsapp";
import { Reveal } from "@/routes/-components/landing/reveal";
import { SectionHeading } from "@/routes/-components/landing/section-heading";

const markerIcon = divIcon({
  className: "",
  html: '<div style="width:20px;height:20px;border-radius:50%;background:#1d4ed8;border:3px solid #fff;box-shadow:0 0 0 2px rgba(0,0,0,0.25)"></div>',
  iconSize: [20, 20],
  iconAnchor: [10, 10],
});

const mapsUrl = (coords: string) =>
  `https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(coords)}`;

export function ContactSection() {
  return (
    <section
      id="contato"
      className="mx-auto w-full max-w-360 px-6 py-24 md:py-28"
    >
      <SectionHeading
        eyebrow="Bases Operacionais"
        title="Bases regionais, atuação nacional."
        lead={
          "Equipes técnicas baseadas na Bahia e em Sergipe.\nAtendemos a todo o Brasil."
        }
        align="center"
      />

      <Reveal className="mt-8 flex flex-wrap items-center justify-center gap-3">
        <a
          href={`mailto:${contact.email}`}
          className={cn(
            buttonVariants({ variant: "secondary", size: "lg" }),
            "h-12 gap-2 px-7 font-mono text-xs font-semibold tracking-wider uppercase",
          )}
        >
          <Mail className="size-4" />
          Solicitar proposta
        </a>
        <a
          href={`https://wa.me/${contact.whatsapp}`}
          target="_blank"
          rel="noopener noreferrer"
          className={cn(
            buttonVariants({ variant: "outline", size: "lg" }),
            "h-12 gap-2 px-7 font-mono text-xs font-semibold tracking-wider uppercase",
          )}
        >
          <WhatsappIcon className="size-4 text-green-500" />
          WhatsApp
        </a>
      </Reveal>

      <div className="mt-14 grid gap-6 sm:grid-cols-2">
        {offices.map((office, index) => {
          const [lat, lng] = office.coords.split(",").map(Number);
          const position = { lat, lng };

          return (
            <Reveal key={office.city} delay={index * 120} className="h-full">
              <Card className="h-full gap-0 py-0 transition-all duration-500 hover:-translate-y-1 hover:shadow-xl hover:ring-secondary/50">
                <div className="relative overflow-hidden">
                  <div className="h-56 grayscale transition-[filter] duration-700 group-hover/card:grayscale-0">
                    <MapContainer
                      key={office.coords}
                      center={position}
                      zoom={14}
                      className="h-full w-full"
                      zoomControl
                      scrollWheelZoom={false}
                      dragging={false}
                    >
                      <TileLayer
                        attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
                        url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
                        referrerPolicy="strict-origin-when-cross-origin"
                      />
                      <Marker position={position} icon={markerIcon} />
                    </MapContainer>
                  </div>
                  <div className="absolute inset-x-0 bottom-0 flex items-end justify-between gap-3 bg-linear-to-t from-accent via-accent/70 to-transparent px-5 pt-12 pb-4">
                    <div className="pointer-events-none min-w-0">
                      <span className="font-mono text-[0.625rem] tracking-[0.2em] text-muted uppercase dark:text-foreground">
                        {office.region}
                      </span>
                      <h3 className="text-2xl font-bold tracking-tight text-white">
                        {office.city}
                        <span className="text-secondary">, {office.state}</span>
                      </h3>
                    </div>

                    <a
                      href="https://www.openstreetmap.org/copyright"
                      target="_blank"
                      rel="noopener noreferrer"
                      className="rounded-md border border-white/15 bg-black/40 px-2 py-1 font-mono text-[0.625rem] tracking-wide text-white/70 backdrop-blur-sm transition-colors hover:text-white"
                    >
                      © OpenStreetMap
                    </a>
                  </div>
                </div>

                <CardFooter>
                  <a
                    href={mapsUrl(office.coords)}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex h-11 w-full items-center justify-center gap-2 rounded-lg border border-border bg-background px-4 font-mono text-xs font-semibold tracking-wider text-foreground uppercase transition-colors hover:border-secondary hover:text-secondary"
                  >
                    <MapPin className="size-4" />
                    Ver no mapa
                    <ArrowUpRight className="size-4" />
                  </a>
                </CardFooter>
              </Card>
            </Reveal>
          );
        })}
      </div>
    </section>
  );
}
