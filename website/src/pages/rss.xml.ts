import rss from "@astrojs/rss";
import { getCollection } from "astro:content";
import { blogSlugFromId } from "../lib/blog-slug";

export async function GET(context: any) {
  const posts = await getCollection("blog");
  return rss({
    title: "ICN Blog",
    description: "InterCooperative Network Updates",
    site: context.site,
    items: posts.map((post) => ({
      title: post.data.title,
      pubDate: post.data.pubDate,
      description: post.data.description,
      link: `/blog/${blogSlugFromId(post.id)}/`,
    })),
  });
}
