import {
  Activity,
  BadgeCheck,
  Cpu,
  Eye,
  Factory,
  Flag,
  FlaskConical,
  Fuel,
  Gem,
  Hammer,
  HardHat,
  Radar,
  Shield,
  Ship,
  Sprout,
  Utensils,
  Wheat,
  Wrench,
  type LucideIcon,
} from "lucide-react";

import { assets } from "@/routes/-components/landing/assets";

export type Consultant = {
  name: string;
  email: string;
  phone: string;
};

export type Office = {
  region: string;
  city: string;
  state: string;
  address: string;
  phone: string;
  email: string;
  coords: string;
};

export type Sector = {
  name: string;
  short: string;
  icon: LucideIcon;
};

export type Pillar = {
  code: string;
  label: string;
  icon: LucideIcon;
  body?: string;
  list?: readonly string[];
  note: string;
};

export type GovernancePrinciple = {
  title: string;
  body: string;
  icon: LucideIcon;
  sigil: string;
};

export type SelectorOption = {
  title: string;
  description: string;
  image: string;
  imageAlt?: string;
  icon: LucideIcon;
  tags?: readonly string[];
};

export const brand = {
  name: "OG Service",
  slogan: "Performance, Segurança e Confiabilidade.",
  shortSlogan: "Performance · Segurança · Confiabilidade",
  legalName: "OG Service",
  cnpj: "XX.XXX.XXX/0001-XX",
} as const;

export const consultants: readonly Consultant[] = [
  {
    name: "Ordálvio Guimarães",
    email: "ordalvio.guimaraes@ogservice.com.br",
    phone: "+55 (82) 99237-7070",
  },
  {
    name: "Dermeval Filho",
    email: "dermeval.filho@ogservice.com.br",
    phone: "+55 (71) 99669-3277",
  },
];

export const contact = {
  email: "comercial@ogservice.com.br",
  phone: "(71) 9 9615-1703",
  whatsapp: "5571996151703",
  instagram: "https://www.instagram.com/ogs.service",
  linkedin: "https://www.linkedin.com/in/lindaflor",
} as const;

export const offices: readonly Office[] = [
  {
    region: "Unidade · Bahia",
    city: "Catu",
    state: "BA",
    address: "Centro",
    phone: contact.phone,
    email: contact.email,
    coords: "-12.35167,-38.37833",
  },
  {
    region: "Unidade · Sergipe",
    city: "Barra dos Coqueiros",
    state: "SE",
    address: "Centro",
    phone: contact.phone,
    email: contact.email,
    coords: "-10.90889,-37.03861",
  },
];

export const sectors: readonly Sector[] = [
  { name: "Óleo & Gás", short: "Óleo & Gás", icon: Fuel },
  { name: "Petroquímica", short: "Petroquímica", icon: FlaskConical },
  { name: "Agronegócio", short: "Agronegócio", icon: Wheat },
  { name: "Indústria de Alimentos", short: "Alimentos", icon: Utensils },
  {
    name: "Portos & Logística Industrial",
    short: "Portos & Logística",
    icon: Ship,
  },
];

export const about = {
  intro:
    "A OG Service nasce com o propósito de oferecer soluções integradas de operação e manutenção industrial para setores estratégicos da economia, atuando principalmente nos segmentos de óleo e gás, petroquímica, agronegócio, indústria de alimentos, portos e logística industrial.",
  body: "A empresa foi concebida para atender à crescente demanda por serviços especializados, confiáveis e eficientes, alinhados às necessidades operacionais de plantas industriais que exigem alta disponibilidade, segurança operacional e desempenho contínuo.",
  model:
    "O modelo de negócio da OG Service é baseado em uma estrutura técnica, enxuta e escalável, com foco em excelência operacional, gestão profissional e atendimento personalizado. A empresa atua por meio de contratos de manutenção industrial, apoio operacional, inspeção, planejamento e controle de manutenção (PCM) e serviços sob demanda, com estratégia gradual de expansão para contratos integrados de operação e manutenção industrial.",
} as const;

const mission =
  "Entregar soluções industriais com segurança, eficiência e confiabilidade, contribuindo para a continuidade operacional e o crescimento sustentável dos nossos parceiros.";

const vision =
  "Ser referência nacional na prestação de serviços especializados de operação, manutenção e inovação tecnológica, reconhecida pela excelência técnica e confiança.";

const values = [
  "Segurança em primeiro lugar.",
  "Ética e transparência.",
  "Excelência operacional.",
  "Compromisso com resultado.",
  "Inovação contínua.",
  "Valorização das pessoas.",
] as const;

export const pillars: readonly Pillar[] = [
  {
    code: "MVV-01",
    label: "Missão",
    icon: Flag,
    body: mission,
    note: "Continuidade operacional",
  },
  {
    code: "MVV-02",
    label: "Visão",
    icon: Eye,
    body: vision,
    note: "Referência nacional",
  },
  {
    code: "MVV-03",
    label: "Valores",
    icon: Gem,
    list: values,
    note: `${String(values.length).padStart(2, "0")} princípios`,
  },
];

export const governance = {
  intro:
    "A OG Service adota um modelo de governança corporativa baseada em princípios de ética, transparência, responsabilidade operacional e excelência industrial. O objetivo é garantir processos estruturados, segurança operacional e conformidade com normas técnicas e regulatórias dos setores de petróleo, gás, petroquímica, agronegócio e portos.",
  principles: [
    {
      title: "Ética e Transparência",
      body: "Atuamos com integridade em todas as relações comerciais, operacionais e institucionais, mantendo comunicação clara com clientes, colaboradores, parceiros e órgãos governamentais.",
      icon: BadgeCheck,
      sigil: "I",
    },
    {
      title: "Segurança Operacional",
      body: "A segurança nas atividades é tratada como valor inegociável, com foco em prevenção de riscos, proteção das pessoas, integridade dos ativos e preservação ambiental.",
      icon: HardHat,
      sigil: "II",
    },
    {
      title: "Compliance e Conformidade",
      body: "Buscamos conformidade com as normas nacionais e internacionais aplicáveis ao setor industrial, incluindo requisitos de SMS (segurança, meio ambiente e saúde), qualidade e gestão operacional.",
      icon: Shield,
      sigil: "III",
    },
    {
      title: "Sustentabilidade",
      body: "Incentivamos práticas sustentáveis, eficiência energética e responsabilidade ambiental em todas as nossas operações e serviços.",
      icon: Sprout,
      sigil: "IV",
    },
  ] satisfies readonly GovernancePrinciple[],
} as const;

export const services: readonly SelectorOption[] = [
  {
    title: "Inspeção com Drones",
    description:
      "Inspeção aérea de ativos elevados, tanques, tochas e estruturas de difícil acesso, com registro fotográfico e termográfico de alta resolução.",
    image: assets.drone,
    imageAlt: "Vista aérea de instalações industriais inspecionadas por drone",
    icon: Radar,
    tags: [
      "Inspeção de tanques, tochas e torres",
      "Termografia aérea",
      "Modelagem 3D de ativos",
      "Áreas de difícil acesso",
    ],
  },
  {
    title: "Manutenção Industrial",
    description:
      "Programas completos para garantir disponibilidade, integridade e desempenho de equipamentos estáticos e rotativos.",
    image: assets.manutencao,
    imageAlt: "Equipe de manutenção industrial em operação",
    icon: Wrench,
    tags: [
      "Manutenção preventiva em equipamentos estáticos e rotativos",
      "Manutenção corretiva em equipamentos estáticos e rotativos",
      "Manutenção preditiva",
      "Paradas programadas",
      "Inspeção industrial",
    ],
  },
  {
    title: "Engenharia e Confiabilidade",
    description:
      "Estratégias de gestão de ativos sustentadas por dados, análise de falhas e indicadores que aumentam a vida útil das plantas.",
    image: assets.engenharia,
    imageAlt: "Profissionais de engenharia avaliando ativos industriais",
    icon: Activity,
    tags: [
      "Segurança de Processo e Ocupacional",
      "Planejamento de manutenção",
      "PCM — Planejamento e Controle de Manutenção",
      "Análise de falhas",
      "Gestão de ativos",
      "Indicadores KPI",
      "Reliability Centered Maintenance (RCM)",
    ],
  },
  {
    title: "Tecnologia e Inovação",
    description:
      "Digitalização aplicada à manutenção: dashboards, inspeção remota e aplicativos operacionais sob medida.",
    image: assets.tech,
    imageAlt: "Interface digital aplicada a operação industrial",
    icon: Cpu,
    tags: [
      "Monitoramento remoto",
      "Digitalização de manutenção",
      "Aplicativos operacionais",
      "Dashboards operacionais",
    ],
  },
  {
    title: "Operação Industrial",
    description:
      "Equipes especializadas em operação contínua de plantas industriais com supervisão e controle de processos.",
    image: assets.plantas,
    imageAlt: "Planta industrial em operação contínua",
    icon: Factory,
    tags: [
      "Operação de plantas industriais",
      "Supervisão operacional",
      "Controle de processos",
    ],
  },
  {
    title: "Engenharia Civil",
    description:
      "Projetos, execução e fiscalização de obras civis com foco em ambientes industriais.",
    image: assets.engenharia2,
    imageAlt: "Obra civil em ambiente industrial",
    icon: Hammer,
    tags: [
      "Projeto de Engenharia",
      "Manutenção predial",
      "Execução de obras civis",
      "Fiscalização de obras civis",
    ],
  },
];

export const nav = [
  { label: "Sobre", href: "#sobre" },
  { label: "Princípios", href: "#principios" },
  { label: "Serviços", href: "#servicos" },
  { label: "Governança", href: "#governanca" },
  { label: "Talentos", href: "#talentos" },
  { label: "Contato", href: "#contato" },
] as const;

export const legalLinks = [
  { label: "Termos de Uso", href: "#" },
  { label: "Política de Privacidade", href: "#" },
  { label: "Compliance", href: "#" },
  { label: "Trabalhe Conosco", href: "/curriculum/submit" },
] as const;

export const landingSeo = {
  siteUrl: "https://www.ogservice.com.br",
  pageUrl: "https://www.ogservice.com.br/",
  title: "OG Service — Performance, Segurança e Confiabilidade",
  description:
    "A OG Service oferece soluções integradas de operação e manutenção industrial para os setores de óleo e gás, petroquímica, agronegócio, indústria de alimentos e portos.",
  ogImage: "https://www.ogservice.com.br/og-image.png",
} as const;
