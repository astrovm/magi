const HTML_HEADERS = { 'Content-Type': 'text/html; charset=utf-8' } as const;
const TEXT_HEADERS = { 'Content-Type': 'text/plain; charset=utf-8' } as const;

const ORB_REJECTED_REQUEST_MESSAGE = 'the orb rejected your request.';
const ORB_REJECTED_ALIAS_MESSAGE = 'the orb rejected your alias.';
const ORB_REJECTED_INVALID_URL_MESSAGE = 'the orb rejected your invalid url.';
const ORB_REJECTED_ALIAS_REWRITE_MESSAGE = 'the orb rejected your access to rewrite this link.';
const ORB_LINK_NOT_FOUND_MESSAGE = "the orb didn't found this link.";
const WORM_LINK_CREATED_MESSAGE = (alias: string) =>
  `the worm summoned your link <a href="https://s.4st.li/${alias}">s.4st.li/${alias}</a>`;

type ResponseKey =
  | 'invalidRequestHtml'
  | 'invalidRequestText'
  | 'invalidAlias'
  | 'invalidUrl'
  | 'aliasLocked'
  | 'linkNotFound'
  | 'linkCreated';

type ResponseContext = {
  alias?: string;
};

type ResponseBuilder = (context?: ResponseContext) => Response;

const createHtmlResponse = (message: string): Response =>
  new Response(`<p>${message}</p>`, { headers: HTML_HEADERS });

const createTextResponse = (message: string): Response =>
  new Response(`${message}\n`, { headers: TEXT_HEADERS });

const RESPONSE_BUILDERS: Record<ResponseKey, ResponseBuilder> = {
  invalidRequestHtml: () => createHtmlResponse(ORB_REJECTED_REQUEST_MESSAGE),
  invalidRequestText: () => createTextResponse(ORB_REJECTED_REQUEST_MESSAGE),
  invalidAlias: () => createHtmlResponse(ORB_REJECTED_ALIAS_MESSAGE),
  invalidUrl: () => createHtmlResponse(ORB_REJECTED_INVALID_URL_MESSAGE),
  aliasLocked: () => createHtmlResponse(ORB_REJECTED_ALIAS_REWRITE_MESSAGE),
  linkNotFound: () => createTextResponse(ORB_LINK_NOT_FOUND_MESSAGE),
  linkCreated: (context?: ResponseContext) => {
    const alias = context?.alias;

    if (!alias) {
      throw new Error('The linkCreated response requires an alias.');
    }

    return createHtmlResponse(WORM_LINK_CREATED_MESSAGE(alias));
  },
};

const getResponse = (key: ResponseKey, context?: ResponseContext): Response =>
  RESPONSE_BUILDERS[key](context);

export {
  getResponse,
  ORB_REJECTED_INVALID_URL_MESSAGE,
  ORB_REJECTED_REQUEST_MESSAGE,
  ORB_REJECTED_ALIAS_MESSAGE,
  ORB_REJECTED_ALIAS_REWRITE_MESSAGE,
  ORB_LINK_NOT_FOUND_MESSAGE,
  HTML_HEADERS,
  TEXT_HEADERS,
};
export type { ResponseKey, ResponseContext };
