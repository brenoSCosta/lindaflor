import { db } from "@lindaflor/db";
import { careers } from "@lindaflor/db/schema/career";
import { curriculums } from "@lindaflor/db/schema/curriculum";
import { ensureBucket, uploadFile } from "@lindaflor/s3";
import { PDF_MIME_TYPE } from "@lindaflor/shared/constants";
import { Effect } from "effect";
import { v7 as uuidv7 } from "uuid";

import { randomSeedDate } from "@/seed/utils";

const SEED_CANDIDATES = [
  {
    id: "0191a000-0000-7000-0000-000000000200",
    name: "Carlos Eduardo Silva",
    email: "carlos.silva@example.com",
    phone: "(71) 98765-4321",
    careerTitle: "Técnico de Operação de Campo",
    summary:
      "Profissional com mais de 8 anos de experiência em operação de equipamentos de coleta e transferência de petróleo e gás. Atuação em campo, manutenção preditiva e corretiva de bombas e válvulas, além de acompanhamento de indicadores de produção.",
    skills: [
      "Operação de campo",
      "NR-10",
      "NR-33",
      "NR-35",
      "Manutenção de bombas",
      "Válvulas industriais",
      "Leitura de P\u0026ID",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-000000000201",
    name: "Ana Paula Mendes",
    email: "ana.mendes@example.com",
    phone: "(21) 99876-5432",
    careerTitle: "Engenheiro de Petróleo",
    summary:
      "Engenheira de Petróleo com experiência em análise de reservatórios, acompanhamento de poços e otimização de produção. Atuou em campos terrestres e offshore, com forte vivência em simulação de reservatórios e integração multidisciplinar.",
    skills: [
      "Análise de reservatórios",
      "Eclipse",
      "Petrel",
      "Poços terrestres",
      "Poços offshore",
      "Otimização de produção",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-000000000202",
    name: "Ricardo Oliveira",
    email: "ricardo.oliveira@example.com",
    phone: "(27) 91234-5678",
    careerTitle: "Inspetor de Solda",
    summary:
      "Inspetor de solda certificado com ampla experiência em ensaios não destrutivos e inspeção de juntas soldadas em tubulações e equipamentos de processo. Atuação offshore e em plantas industriais.",
    skills: [
      "Inspeção de solda",
      "END",
      "Ultrassom",
      "Partículas magnéticas",
      "Líquido penetrante",
      "ASME",
      "AWS D1.1",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-000000000203",
    name: "Fernanda Lima",
    email: "fernanda.lima@example.com",
    phone: "(13) 97654-3210",
    careerTitle: "Coordenador de Logística Offshore",
    summary:
      "Coordenadora de logística offshore com sólida experiência no planejamento e execução de embarques de pessoal, materiais e equipamentos para unidades offshore. Conhecimento em legislação portuária e regulamentação da ANP.",
    skills: [
      "Logística offshore",
      "Planejamento de embarques",
      "ANP",
      "Legislação portuária",
      "SAP",
      "Gestão de frota",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-000000000204",
    name: "João Pedro Souza",
    email: "joao.souza@example.com",
    phone: "(79) 98888-9999",
    careerTitle: "Operador de Planta de Processamento",
    summary:
      "Operador de planta de processamento com experiência em operação contínua de plantas de petróleo, gás e derivados. Vivência em painéis de controle, rondas operacionais e procedimentos de emergência.",
    skills: [
      "Operação de planta",
      "Controle de processos",
      "Rondas operacionais",
      "Procedimentos de emergência",
      "Compressor",
      "Separação",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-000000000205",
    name: "Mariana Costa",
    email: "mariana.costa@example.com",
    phone: "(22) 97777-6666",
    careerTitle: "Engenheiro de Processos",
    summary:
      "Engenheira de processos com experiência em desenvolvimento e otimização de processos industriais para separação, tratamento e transferência de petróleo e gás natural. Domínio em simulação de processos e normas técnicas.",
    skills: [
      "HYSYS",
      "PIPESIM",
      "Simulação de processos",
      "API",
      "DNV",
      "Otimização energética",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-000000000206",
    name: "Bruno Henrique Alves",
    email: "bruno.alves@example.com",
    phone: "(75) 96666-5555",
    careerTitle: "Técnico em Mecânica Industrial",
    summary:
      "Técnico em Mecânica Industrial com experiência em manutenção preventiva e corretiva de bombas, compressores, válvulas e equipamentos rotativos. Conhecimento em alinhamento, balanceamento e lubrificação.",
    skills: [
      "Manutenção mecânica",
      "Bombas",
      "Compressores",
      "Alinhamento",
      "Balanceamento",
      "Lubrificação",
      "CMMS",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-000000000207",
    name: "Larissa Martins",
    email: "larissa.martins@example.com",
    phone: "(84) 95555-4444",
    careerTitle: "Supervisor de Segurança do Trabalho",
    summary:
      "Supervisora de segurança do trabalho com experiência em gestão de segurança, saúde ocupacional e meio ambiente em instalações operacionais. Liderança de equipes HSE e implantação de programas de prevenção.",
    skills: [
      "Gestão de HSE",
      "NR-05",
      "NR-06",
      "NR-33",
      "NR-35",
      "Investigação de acidentes",
      "Liderança",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-000000000208",
    name: "Tiago Ferreira",
    email: "tiago.ferreira@example.com",
    phone: "(51) 94444-3333",
    careerTitle: "Analista de Manutenção Predial",
    summary:
      "Analista de manutenção predial com experiência em planejamento e acompanhamento de atividades de manutenção predial e infraestrutura. Gestão de contratos e conhecimento em SAP.",
    skills: [
      "Manutenção predial",
      "Gestão de contratos",
      "SAP",
      "Planejamento de manutenção",
      "Orçamento",
      "Gestão de fornecedores",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-000000000209",
    name: "Camila Rocha",
    email: "camila.rocha@example.com",
    phone: "(11) 93333-2222",
    careerTitle: "Engenheiro de Inspeção",
    summary:
      "Engenheira de inspeção com experiência em inspeções de equipamentos e tubulações conforme normas técnicas e regulatórias. Especialista em vasos de pressão, tubulações e ensaios não destrutivos.",
    skills: [
      "Inspeção de equipamentos",
      "Vasos de pressão",
      "Tubulações",
      "ASME",
      "API 510",
      "API 570",
      "END",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-00000000020a",
    name: "Gabriel Santos",
    email: "gabriel.santos@example.com",
    phone: "(31) 92222-1111",
    careerTitle: "Estagiário de Engenharia de Petróleo",
    summary:
      "Estagiário de Engenharia de Petróleo no 7º período, com interesse em análises técnicas, relatórios de poços e acompanhamento de dados operacionais. Conhecimento em Excel e ferramentas de análise de dados.",
    skills: [
      "Excel avançado",
      "Análise de dados",
      "Relatórios técnicos",
      "Poços",
      "Reservatórios",
      "Inglês técnico",
    ],
  },
  {
    id: "0191a000-0000-7000-0000-00000000020b",
    name: "Patrícia Duarte",
    email: "patricia.duarte@example.com",
    phone: "(41) 91111-0000",
    careerTitle: "Estagiário de Tecnologia da Informação",
    summary:
      "Estagiária de TI no 5º período de Ciência da Computação, com interesse em suporte técnico, automações e gestão de infraestrutura. Proativa e com boa comunicação.",
    skills: [
      "Suporte técnico",
      "Redes",
      "Sistemas operacionais",
      "Automação",
      "Python",
      "Linux",
    ],
  },
] as const;

function createFakePdfBuffer(fileName: string, submittedAt: Date): Buffer {
  const creationDate = submittedAt.toISOString();
  const content = `%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>
endobj
4 0 obj
<< /Length ${100 + fileName.length + creationDate.length} >>
stream
BT
/F1 24 Tf
100 700 Td
(${fileName}) Tj
0 -36 Td
/F1 12 Tf
(Currículo enviado em ${creationDate}) Tj
ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
xref
0 6
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
0000000266 00000 n 
0000000470 00000 n 
trailer
<< /Size 6 /Root 1 0 R >>
startxref
548
%%EOF`;
  return Buffer.from(content, "utf-8");
}

function slugifyFileName(name: string): string {
  return name
    .toLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
}

export async function seedCurriculum() {
  const careerRows = await db
    .select({ id: careers.id, title: careers.title })
    .from(careers);
  const careerByTitle = new Map(careerRows.map((row) => [row.title, row.id]));

  ensureBucket();

  for (const candidate of SEED_CANDIDATES) {
    const submittedAt = randomSeedDate();
    const fileName = `curriculo_${slugifyFileName(candidate.name)}.pdf`;
    const fileKey = `curriculums/${uuidv7()}/${fileName}`;
    const buffer = createFakePdfBuffer(fileName, submittedAt);

    await uploadFile(fileKey, buffer, PDF_MIME_TYPE);

    await db.insert(curriculums).values({
      id: candidate.id,
      name: candidate.name,
      email: candidate.email,
      phone: candidate.phone,
      headline: candidate.careerTitle,
      summary: candidate.summary,
      skills: [...candidate.skills],
      career_id: careerByTitle.get(candidate.careerTitle) ?? null,
      file_key: fileKey,
      file_name: fileName,
      file_size: buffer.length,
      mime_type: PDF_MIME_TYPE,
      submitted_at: submittedAt,
    });
  }

  Effect.runSync(
    Effect.log(`  curriculum: done (${SEED_CANDIDATES.length} rows)`),
  );
}
