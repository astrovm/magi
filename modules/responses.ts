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

type ResponseFormatter = (message: string) => string;

const formatAsHtml: ResponseFormatter = (message) => `<p>${message}</p>`;
const formatAsText: ResponseFormatter = (message) => `${message}\n`;

const createResponse = (
  message: string,
  formatter: ResponseFormatter,
  headers: HeadersInit,
): Response => new Response(formatter(message), { headers });

const RESPONSE_BUILDERS: Record<ResponseKey, ResponseBuilder> = {
  invalidRequestHtml: () => createResponse(ORB_REJECTED_REQUEST_MESSAGE, formatAsHtml, HTML_HEADERS),
  invalidRequestText: () => createResponse(ORB_REJECTED_REQUEST_MESSAGE, formatAsText, TEXT_HEADERS),
  invalidAlias: () => createResponse(ORB_REJECTED_ALIAS_MESSAGE, formatAsHtml, HTML_HEADERS),
  invalidUrl: () => createResponse(ORB_REJECTED_INVALID_URL_MESSAGE, formatAsHtml, HTML_HEADERS),
  aliasLocked: () => createResponse(ORB_REJECTED_ALIAS_REWRITE_MESSAGE, formatAsHtml, HTML_HEADERS),
  linkNotFound: () => createResponse(ORB_LINK_NOT_FOUND_MESSAGE, formatAsText, TEXT_HEADERS),
  linkCreated: (context?: ResponseContext) => {
    const alias = context?.alias;

    if (!alias) {
      throw new Error('The linkCreated response requires an alias.');
    }

    return createResponse(WORM_LINK_CREATED_MESSAGE(alias), formatAsHtml, HTML_HEADERS);
  },
};

const getResponse = (key: ResponseKey, context?: ResponseContext): Response =>
  RESPONSE_BUILDERS[key](context);

export {
  createResponse,
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
