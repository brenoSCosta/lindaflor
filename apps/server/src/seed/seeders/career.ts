import { db } from "@lindaflor/db";
import { careers } from "@lindaflor/db/schema/career";
import { Effect } from "effect";

import { randomSeedDate, seedIdFor } from "@/seed/utils";

export const SEED_CAREERS = [
  {
    title: "Técnico de Operação de Campo",
    department: "Operações",
    location: "Madre de Deus - BA",
    type: "CLT",
    description:
      "Atuação em campo na operação de equipamentos de coleta e transferência de petróleo e gás.",
    requirements: [
      "Ensino técnico completo em Mecânica, Eletrotécnica ou áreas correlatas",
      "Disponibilidade para trabalho em escala offshore",
      "NR-10, NR-33 e NR-35 atualizadas",
    ],
  },
  {
    title: "Engenheiro de Petróleo",
    department: "Engenharia",
    location: "Candeias - BA",
    type: "CLT",
    description:
      "Atuação no planejamento e acompanhamento de operações de produção de petróleo e gás, análise de reservatórios e otimização de processos.",
    requirements: [
      "Engenharia de Petróleo completa",
      "CREA ativo",
      "Experiência em operadoras de óleo e gás ou empresas de serviços petrolíferos",
    ],
  },
  {
    title: "Engenheiro de Inspeção",
    department: "Engenharia",
    location: "São Mateus - ES",
    type: "CLT",
    description:
      "Responsável por inspeções de equipamentos e tubulações conforme normas técnicas e regulatórias.",
    requirements: [
      "Engenharia Mecânica, Metalúrgica ou Química completa",
      "CREA ativo",
      "Experiência com inspeção de vasos de pressão e tubulações",
    ],
  },
  {
    title: "Engenheiro de Processos",
    department: "Engenharia",
    location: "Macaé - RJ",
    type: "CLT",
    description:
      "Desenvolvimento e otimização de processos industriais para separação, tratamento e transferência de petróleo e gás natural.",
    requirements: [
      "Engenharia Química, de Petróleo ou Mecânica completa",
      "Experiência com simulação de processos (HYSYS, PIPESIM ou similar)",
      "Conhecimento em normas API e DNV",
    ],
  },
  {
    title: "Operador de Planta de Processamento",
    department: "Operações",
    location: "Carmópolis - SE",
    type: "CLT",
    description:
      "Operação contínua de plantas de processamento de petróleo, gás e derivados, acompanhando painéis e realizando rondas operacionais.",
    requirements: [
      "Ensino médio completo; técnico em Química ou Operação de Planta preferencial",
      "Disponibilidade para regime de plantão 12x36",
      "Conhecimento em operações de separação, compressão e medição",
    ],
  },
  {
    title: "Analista de Manutenção Predial",
    department: "Manutenção",
    location: "Carmópolis - SE",
    type: "CLT",
    description:
      "Planejamento e acompanhamento de atividades de manutenção predial e infraestrutura.",
    requirements: [
      "Ensino superior completo em Engenharia Civil, Mecânica ou áreas correlatas",
      "Experiência com gestão de contratos de manutenção",
      "Conhecimento em SAP ou sistema equivalente",
    ],
  },
  {
    title: "Técnico em Mecânica Industrial",
    department: "Manutenção",
    location: "São João da Barra - RJ",
    type: "PJ",
    description:
      "Manutenção preventiva e corretiva de bombas, compressores, válvulas e equipamentos rotativos da planta.",
    requirements: [
      "Técnico em Mecânica completo",
      "Experiência com manutenção de equipamentos em plantas industriais",
      "Conhecimento em alinhamento, balanceamento e lubrificação",
    ],
  },
  {
    title: "Inspetor de Solda",
    department: "Inspeção",
    location: "Macaé - RJ",
    type: "PJ",
    description:
      "Inspeção visual e ensaios não destrutivos em juntas soldadas de tubulações e equipamentos de processo.",
    requirements: [
      "Curso de Inspetor de Solda (CSWIP, AWS CWI ou similar)",
      "Experiência com END (ultrassom, líquido penetrante, partículas magnéticas)",
      "Disponibilidade para trabalho offshore e em campo",
    ],
  },
  {
    title: "Estagiário de Tecnologia da Informação",
    department: "Tecnologia",
    location: "Salvador - BA",
    type: "Estágio",
    description:
      "Apoio à equipe de TI em suporte técnico, automações e gestão de infraestrutura.",
    requirements: [
      "Cursando Ensino Superior em Ciência da Computação, Sistemas de Informação ou áreas correlatas",
      "Conhecimento básico em redes e sistemas operacionais",
      "Boa comunicação e proatividade",
    ],
  },
  {
    title: "Estagiário de Engenharia de Petróleo",
    department: "Engenharia",
    location: "Rio de Janeiro - RJ",
    type: "Estágio",
    description:
      "Apoio às equipes de produção e reservatório em análises técnicas, relatórios de poços e acompanhamento de dados operacionais.",
    requirements: [
      "Cursando Engenharia de Petróleo, Química ou Mecânica a partir do 5º período",
      "Conhecimento em Excel e ferramentas de análise de dados",
      "Inglês técnico intermediário",
    ],
  },
  {
    title: "Supervisor de Segurança do Trabalho",
    department: "Segurança",
    location: "Rio Grande do Norte",
    type: "CLT",
    description:
      "Gestão de segurança, saúde ocupacional e meio ambiente em instalações operacionais.",
    requirements: [
      "Curso técnico ou superior em Segurança do Trabalho",
      "Registro no MTE",
      "Experiência em liderança de equipes de HSE",
    ],
  },
  {
    title: "Coordenador de Logística Offshore",
    department: "Logística",
    location: "Vitória - ES",
    type: "CLT",
    description:
      "Coordenação do transporte de pessoal, materiais e equipamentos para plataformas e unidades offshore.",
    requirements: [
      "Ensino superior completo em Engenharia, Logística ou áreas correlatas",
      "Experiência com logística offshore e embarcações de apoio",
      "Conhecimento em legislação portuária e regulamentação da ANP",
    ],
  },
] as const;

export async function seedCareers() {
  await db.insert(careers).values(
    SEED_CAREERS.map((career) => {
      const createdAt = randomSeedDate();
      return {
        id: seedIdFor(createdAt),
        title: career.title,
        department: career.department,
        location: career.location,
        type: career.type,
        description: career.description,
        requirements: [...career.requirements],
        is_active: true,
        created_at: createdAt,
      };
    }),
  );

  Effect.runSync(Effect.log(`  careers: done (${SEED_CAREERS.length} rows)`));
}
