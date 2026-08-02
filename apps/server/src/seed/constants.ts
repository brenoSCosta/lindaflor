import { env } from "@lindaflor/env/server";
import { v7 as uuidv7 } from "uuid";

export const SEED_COUNT = env.SEED_COUNT ?? 5000;

/**
 * Shared date window for all seeders. Every created_at/updated_at is spread
 * across [SEED_WINDOW_START, SEED_NOW] so dev data realistically spans history.
 *
 * The window is sized by SEED_COUNT (1 hour per row) but never shorter than
 * 6 months (~183 days), satisfying the "at least 6 months before today" rule.
 */
const MS_PER_HOUR = 3_600_000;
const MS_PER_DAY = 86_400_000;
const SIX_MONTHS_MS = MS_PER_DAY * 183;

export const SEED_NOW = new Date();
export const SEED_WINDOW_MS = Math.max(SIX_MONTHS_MS, SEED_COUNT * MS_PER_HOUR);
export const SEED_WINDOW_START = new Date(SEED_NOW.getTime() - SEED_WINDOW_MS);
export const SEED_CLIENT_TIMEZONE = "America/Sao_Paulo";

/**
 * Fixed UUIDs for dev seed data.
 * Dev password: set SEED_DEV_PASSWORD env var or use default (dev only).
 */
export const SEED_ORG_1_ID = "0191a000-0000-7000-0000-000000000001";
export const SEED_ORG_2_ID = "0191a000-0000-7000-0000-000000000002";
export const SEED_ORG_3_ID = "0191a000-0000-7000-0000-000000000003";
export const SEED_ORG_ADMIN_ID = "0191a000-0000-7000-0000-000000000004";

/** Fixed training rows for Playwright specs (Admin Organization). */
export const SEED_E2E_ADMIN_USER_ID = "0191a000-0000-7000-0000-000000000010";
export const SEED_E2E_TRAINING_INTRO_COURSE_ID =
  "0191a000-0000-7000-0000-000000000501";
export const SEED_E2E_TRAINING_GESTAO_COURSE_ID =
  "0191a000-0000-7000-0000-000000000502";
export const SEED_E2E_TRAINING_INTRO_SECTION_ID =
  "0191a000-0000-7000-0000-000000000511";
export const SEED_E2E_TRAINING_INTRO_MODULE_ID =
  "0191a000-0000-7000-0000-000000000521";
export const SEED_E2E_TRAINING_INTRO_LECTURE_1_ID =
  "0191a000-0000-7000-0000-000000000531";
export const SEED_E2E_TRAINING_INTRO_LECTURE_2_ID =
  "0191a000-0000-7000-0000-000000000532";
export const SEED_E2E_TRAINING_INTRO_LECTURE_3_ID =
  "0191a000-0000-7000-0000-000000000533";
export const SEED_E2E_TRAINING_INTRO_QUIZ_ID =
  "0191a000-0000-7000-0000-000000000541";
export const SEED_E2E_TRAINING_INTRO_QUESTION_ID =
  "0191a000-0000-7000-0000-000000000551";
export const SEED_E2E_TRAINING_INTRO_OPTION_CORRECT_ID =
  "0191a000-0000-7000-0000-000000000561";
export const SEED_E2E_TRAINING_INTRO_OPTION_WRONG_ID =
  "0191a000-0000-7000-0000-000000000562";
export const SEED_E2E_TRAINING_INTRO_ENROLLMENT_ID =
  "0191a000-0000-7000-0000-000000000571";

export const SEED_ORG_IDS = [
  SEED_ORG_1_ID,
  SEED_ORG_2_ID,
  SEED_ORG_3_ID,
  SEED_ORG_ADMIN_ID,
] as const;

export const SEED_ORGANIZATIONS = [
  {
    id: SEED_ORG_1_ID,
    name: "Org Alpha",
    slug: "org-alpha",
    logo: "https://placehold.co/200x200/3b82f6/ffffff?text=OA",
    created_at: SEED_WINDOW_START,
    metadata: null,
  },
  {
    id: SEED_ORG_2_ID,
    name: "Org Beta",
    slug: "org-beta",
    logo: "https://placehold.co/200x200/10b981/ffffff?text=OB",
    created_at: SEED_WINDOW_START,
    metadata: null,
  },
  {
    id: SEED_ORG_3_ID,
    name: "Org Gamma",
    slug: "org-gamma",
    logo: "https://placehold.co/200x200/f59e0b/ffffff?text=OG",
    created_at: SEED_WINDOW_START,
    metadata: null,
  },
  {
    id: SEED_ORG_ADMIN_ID,
    name: "Admin Organization",
    slug: "admin-org",
    logo: "https://placehold.co/200x200/ef4444/ffffff?text=AO",
    created_at: SEED_WINDOW_START,
    metadata: null,
  },
];

export const DEV_USERS = [
  {
    id: "0191a000-0000-7000-0000-000000000010",
    email: "admin@lindaflor.com",
    name: "Admin User",
    image: "https://i.pravatar.cc/150?img=1",
    role: "admin" as const,
    organizationId: SEED_ORG_ADMIN_ID,
    orgRole: "owner" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000020",
    email: "moderator@lindaflor.com",
    name: "Moderator User",
    image: "https://i.pravatar.cc/150?img=2",
    role: "moderator" as const,
    organizationId: SEED_ORG_ADMIN_ID,
    orgRole: "member" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000021",
    email: "admin-org-supervisor@lindaflor.com",
    name: "Admin Org Supervisor",
    image: "https://i.pravatar.cc/150?img=18",
    role: "user" as const,
    organizationId: SEED_ORG_ADMIN_ID,
    orgRole: "supervisor" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000011",
    email: "org-alpha-owner@lindaflor.com",
    name: "Org Alpha Owner",
    image: "https://i.pravatar.cc/150?img=3",
    role: "user" as const,
    organizationId: SEED_ORG_1_ID,
    orgRole: "owner" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000012",
    email: "org-alpha-admin@lindaflor.com",
    name: "Org Alpha Admin",
    image: "https://i.pravatar.cc/150?img=4",
    role: "user" as const,
    organizationId: SEED_ORG_1_ID,
    orgRole: "admin" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000013",
    email: "org-alpha-operator@lindaflor.com",
    name: "Org Alpha Operator",
    image: "https://i.pravatar.cc/150?img=5",
    role: "user" as const,
    organizationId: SEED_ORG_1_ID,
    orgRole: "operator" as const,
  },
  {
    id: "0191a000-0000-7000-0000-00000000001d",
    email: "org-alpha-supervisor@lindaflor.com",
    name: "Org Alpha Supervisor",
    image: "https://i.pravatar.cc/150?img=11",
    role: "user" as const,
    organizationId: SEED_ORG_1_ID,
    orgRole: "supervisor" as const,
  },
  {
    id: "0191a000-0000-7000-0000-00000000001a",
    email: "org-alpha-member@lindaflor.com",
    name: "Org Alpha Member",
    image: "https://i.pravatar.cc/150?img=6",
    role: "user" as const,
    organizationId: SEED_ORG_1_ID,
    orgRole: "member" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000014",
    email: "org-beta-owner@lindaflor.com",
    name: "Org Beta Owner",
    image: "https://i.pravatar.cc/150?img=7",
    role: "user" as const,
    organizationId: SEED_ORG_2_ID,
    orgRole: "owner" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000015",
    email: "org-beta-admin@lindaflor.com",
    name: "Org Beta Admin",
    image: "https://i.pravatar.cc/150?img=8",
    role: "user" as const,
    organizationId: SEED_ORG_2_ID,
    orgRole: "admin" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000016",
    email: "org-beta-operator@lindaflor.com",
    name: "Org Beta Operator",
    image: "https://i.pravatar.cc/150?img=9",
    role: "user" as const,
    organizationId: SEED_ORG_2_ID,
    orgRole: "operator" as const,
  },
  {
    id: "0191a000-0000-7000-0000-00000000001e",
    email: "org-beta-supervisor@lindaflor.com",
    name: "Org Beta Supervisor",
    image: "https://i.pravatar.cc/150?img=12",
    role: "user" as const,
    organizationId: SEED_ORG_2_ID,
    orgRole: "supervisor" as const,
  },
  {
    id: "0191a000-0000-7000-0000-00000000001b",
    email: "org-beta-member@lindaflor.com",
    name: "Org Beta Member",
    image: "https://i.pravatar.cc/150?img=10",
    role: "user" as const,
    organizationId: SEED_ORG_2_ID,
    orgRole: "member" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000017",
    email: "org-gamma-owner@lindaflor.com",
    name: "Org Gamma Owner",
    image: "https://i.pravatar.cc/150?img=13",
    role: "user" as const,
    organizationId: SEED_ORG_3_ID,
    orgRole: "owner" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000018",
    email: "org-gamma-admin@lindaflor.com",
    name: "Org Gamma Admin",
    image: "https://i.pravatar.cc/150?img=14",
    role: "user" as const,
    organizationId: SEED_ORG_3_ID,
    orgRole: "admin" as const,
  },
  {
    id: "0191a000-0000-7000-0000-000000000019",
    email: "org-gamma-operator@lindaflor.com",
    name: "Org Gamma Operator",
    image: "https://i.pravatar.cc/150?img=15",
    role: "user" as const,
    organizationId: SEED_ORG_3_ID,
    orgRole: "operator" as const,
  },
  {
    id: "0191a000-0000-7000-0000-00000000001f",
    email: "org-gamma-supervisor@lindaflor.com",
    name: "Org Gamma Supervisor",
    image: "https://i.pravatar.cc/150?img=17",
    role: "user" as const,
    organizationId: SEED_ORG_3_ID,
    orgRole: "supervisor" as const,
  },
  {
    id: "0191a000-0000-7000-0000-00000000001c",
    email: "org-gamma-member@lindaflor.com",
    name: "Org Gamma Member",
    image: "https://i.pravatar.cc/150?img=16",
    role: "user" as const,
    organizationId: SEED_ORG_3_ID,
    orgRole: "member" as const,
  },
];

export const SEED_STATES = [
  "Bahia",
  "Sergipe",
  "Rio Grande do Norte",
  "Espirito Santo",
  "Rio de Janeiro",
  "Amazonas",
] as const;

export type SeedConcession = {
  id: string;
  name: string;
  state: (typeof SEED_STATES)[number];
};

export const SEED_CONCESSIONS: SeedConcession[] = [
  { id: uuidv7(), name: "Concessao Reconcavo", state: "Bahia" },
  { id: uuidv7(), name: "Concessao Carmopolis", state: "Sergipe" },
  { id: uuidv7(), name: "Concessao Potiguar", state: "Rio Grande do Norte" },
  { id: uuidv7(), name: "Concessao Fazenda Belem", state: "Espirito Santo" },
];

export type SeedInstallation = {
  id: string;
  name: string;
  concession_id: string;
};

export const SEED_INSTALLATIONS: SeedInstallation[] = [
  { id: uuidv7(), name: "Estacao Coletora Alfa", concession_id: "" },
  { id: uuidv7(), name: "Estacao Coletora Bravo", concession_id: "" },
  { id: uuidv7(), name: "Terminal Madre de Deus", concession_id: "" },
  { id: uuidv7(), name: "Unidade de Tratamento Norte", concession_id: "" },
];

export const BATCH_SIZE = 500;

export const LOREM_WORDS = [
  "lorem",
  "ipsum",
  "dolor",
  "sit",
  "amet",
  "consectetur",
  "adipiscing",
  "elit",
  "sed",
  "do",
  "eiusmod",
  "tempor",
  "incididunt",
  "ut",
  "labore",
  "et",
  "dolore",
  "magna",
  "aliqua",
  "enim",
  "ad",
  "minim",
  "veniam",
  "quis",
  "nostrud",
  "exercitation",
  "ullamco",
  "laboris",
  "nisi",
  "aliquip",
  "ex",
  "ea",
  "commodo",
  "consequat",
  "duis",
  "aute",
  "irure",
  "dolor",
  "in",
  "reprehenderit",
  "voluptate",
  "velit",
  "esse",
  "cillum",
  "fugiat",
  "nulla",
  "pariatur",
  "excepteur",
  "sint",
  "occaecat",
  "cupidatat",
  "non",
  "proident",
  "sunt",
  "culpa",
  "qui",
  "officia",
  "deserunt",
  "mollit",
  "anim",
  "id",
  "est",
  "laborum",
];
