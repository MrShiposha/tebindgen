#ifndef _API_H_
#define _API_H_

#ifdef MSVC
#   define API __declspec(dllexport)
#else
#   define API __attribute__((visibility("default")))
#endif // __declspec(dllexport) 

#endif // _API_H_