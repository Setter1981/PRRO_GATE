using System;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

public class WordWord
{
	internal int LL;

	internal string[] TextD;

	private int tdL;

	public WordWord()
	{
		LL = 27;
		tdL = 0;
	}

	internal bool ParsingS(string eS)
	{
		bool result;
		checked
		{
			try
			{
				string[] array = eS.Split(new char[1] { ' ' });
				string text = "";
				int num = array.Length - 1;
				for (int i = 0; i <= num; i++)
				{
					if (array[i].Trim().Length <= 0)
					{
						continue;
					}
					string text2 = array[i].Trim();
					string text3 = text2[0].ToString();
					int num2 = ((!((Operators.CompareString(text3, "<", false) == 0) | (Operators.CompareString(text3, ">", false) == 0))) ? (text.Length + text2.Length + 1) : (text.Length + text2.Length));
					if ((Operators.CompareString(text3, "<", false) == 0) | (Operators.CompareString(text3, ">", false) == 0))
					{
						if (text.Length > 0)
						{
							ref string[] textD = ref TextD;
							textD = (string[])Utils.CopyArray((Array)textD, (Array)new string[tdL + 1]);
							TextD[tdL] = text.Trim();
							text = text2;
							tdL++;
						}
						else
						{
							text = text2;
						}
					}
					else if (num2 <= LL)
					{
						text = text + " " + text2;
					}
					else
					{
						ref string[] textD2 = ref TextD;
						textD2 = (string[])Utils.CopyArray((Array)textD2, (Array)new string[tdL + 1]);
						TextD[tdL] = text.Trim();
						text = text2;
						tdL++;
					}
				}
				if (text.Trim().Length > 0)
				{
					ref string[] textD3 = ref TextD;
					textD3 = (string[])Utils.CopyArray((Array)textD3, (Array)new string[tdL + 1]);
					TextD[tdL] = text.Trim();
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
				goto IL_01e7;
			}
			result = LineAAlignment();
			goto IL_01e7;
		}
		IL_01e7:
		return result;
	}

	private bool LineAAlignment()
	{
		checked
		{
			int num = TextD.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				string text = TextD[i].Trim();
				if (text.Length > 0)
				{
					if (Operators.CompareString(text[0].ToString(), "<", false) == 0)
					{
						text = text.Substring(1, text.Length - 1);
						TextD[i] = text + Strings.Space(LL - text.Length);
					}
					else if (Operators.CompareString(text[0].ToString(), ">", false) == 0)
					{
						text = text.Substring(1, text.Length - 1);
						TextD[i] = Strings.Space(LL - text.Length) + text;
					}
				}
			}
			return true;
		}
	}

	internal string CenterAlignment(string e)
	{
		string text = e.Trim();
		checked
		{
			if (text.Length > LL)
			{
				text = text.Substring(0, LL);
			}
			else if (text.Length < LL)
			{
				if (text.Length > 0)
				{
					text = Strings.Space((int)Math.Round((double)(LL - text.Length) / 2.0)) + text;
					text += Strings.Space(LL - text.Length);
				}
				else
				{
					text = Strings.Space(LL);
				}
			}
			return text;
		}
	}
}
