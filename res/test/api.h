#ifndef _API_H_
#define _API_H_

#ifdef MSVC
#   define API __declspec(dllexport)
#   define HIDDEN
#else
#   define API __attribute__((visibility("default")))
#   define HIDDEN __attribute__((visibility("hidden")))
#endif // __declspec(dllexport) 

#endif // _API_H_