import { LandingFooter } from "@/routes/-components/landing/landing-footer";
import { useRevealEnabled } from "@/routes/-components/landing/reveal";
import { AboutSection } from "@/routes/-components/landing/sections/about";
import { ContactSection } from "@/routes/-components/landing/sections/contact";
import { GovernanceSection } from "@/routes/-components/landing/sections/governance";
import { HeroSection } from "@/routes/-components/landing/sections/hero";
import { MissionVisionValuesSection } from "@/routes/-components/landing/sections/mission-vision-values";
import { ServicesSection } from "@/routes/-components/landing/sections/services";
import { TalentPoolSection } from "@/routes/-components/landing/sections/talent-pool";

export function LandingPage() {
  useRevealEnabled();

  return (
    <div className="industrial-grid">
      <HeroSection />
      <AboutSection />
      <MissionVisionValuesSection />
      <ServicesSection />
      <GovernanceSection />
      <TalentPoolSection />
      <ContactSection />
      <LandingFooter />
    </div>
  );
}
