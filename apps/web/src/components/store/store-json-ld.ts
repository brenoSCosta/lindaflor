const STORE_URL = "https://lindaflor.com.br";

export const storeJsonLd = {
  organization: () => ({
    "@context": "https://schema.org",
    "@type": "Organization",
    name: "Linda Flor Moda Praia",
    url: STORE_URL,
    logo: `${STORE_URL}/apple-touch-icon.png`,
    sameAs: ["https://instagram.com/BIQUINISLINDAFLOR"],
    address: {
      "@type": "PostalAddress",
      streetAddress: "Rua Capitão Isaias Alves de Souza, 1100",
      addressLocality: "Aracaju",
      addressRegion: "SE",
      addressCountry: "BR",
    },
    contactPoint: {
      "@type": "ContactPoint",
      telephone: "+55-79-99816-5115",
      contactType: "customer service",
      availableLanguage: "Portuguese",
    },
  }),

  website: () => ({
    "@context": "https://schema.org",
    "@type": "WebSite",
    name: "Linda Flor Moda Praia",
    url: STORE_URL,
    potentialAction: {
      "@type": "SearchAction",
      target: `${STORE_URL}/produtos?q={search_term_string}`,
      "query-input": "required name=search_term_string",
    },
  }),

  product: (params: {
    name: string;
    description: string | null;
    slug: string;
    imageUrl: string | null;
    priceCents: number;
    inStock: boolean;
  }) => ({
    "@context": "https://schema.org",
    "@type": "Product",
    name: params.name,
    description: params.description ?? undefined,
    image: params.imageUrl ?? undefined,
    url: `${STORE_URL}/produtos/${params.slug}`,
    brand: {
      "@type": "Brand",
      name: "Linda Flor",
    },
    offers: {
      "@type": "Offer",
      priceCurrency: "BRL",
      price: (params.priceCents / 100).toFixed(2),
      availability: params.inStock
        ? "https://schema.org/InStock"
        : "https://schema.org/OutOfStock",
      url: `${STORE_URL}/produtos/${params.slug}`,
    },
  }),
};
