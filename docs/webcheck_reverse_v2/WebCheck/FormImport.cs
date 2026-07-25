using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Dynamic;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Newtonsoft.Json;

namespace WebCheck;

[DesignerGenerated]
internal class FormImport : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ListBox1")]
	private CheckedListBox _ListBox1;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("SeaB")]
	private Button _SeaB;

	private int InfaC;

	private TypNumFiscal[] Infa;

	private string JS;

	[field: AccessedThroughProperty("TextBox1")]
	internal virtual TextBox TextBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckedListBox ListBox1
	{
		[CompilerGenerated]
		get
		{
			return _ListBox1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ListBox1_SelectedIndexChanged;
			CheckedListBox listBox = _ListBox1;
			if (listBox != null)
			{
				listBox.SelectedIndexChanged -= value2;
			}
			_ListBox1 = value;
			listBox = _ListBox1;
			if (listBox != null)
			{
				listBox.SelectedIndexChanged += value2;
			}
		}
	}

	internal virtual Button OkB
	{
		[CompilerGenerated]
		get
		{
			return _OkB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				okB.Click -= value2;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				okB.Click += value2;
			}
		}
	}

	internal virtual Button NoB
	{
		[CompilerGenerated]
		get
		{
			return _NoB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				noB.Click -= value2;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				noB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("SeaT")]
	internal virtual TextBox SeaT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button SeaB
	{
		[CompilerGenerated]
		get
		{
			return _SeaB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = SeaB_Click;
			Button seaB = _SeaB;
			if (seaB != null)
			{
				seaB.Click -= value2;
			}
			_SeaB = value;
			seaB = _SeaB;
			if (seaB != null)
			{
				seaB.Click += value2;
			}
		}
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormImport));
		this.TextBox1 = new System.Windows.Forms.TextBox();
		this.ListBox1 = new System.Windows.Forms.CheckedListBox();
		this.OkB = new System.Windows.Forms.Button();
		this.NoB = new System.Windows.Forms.Button();
		this.SeaT = new System.Windows.Forms.TextBox();
		this.SeaB = new System.Windows.Forms.Button();
		base.SuspendLayout();
		this.TextBox1.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.TextBox1.Enabled = false;
		this.TextBox1.Font = new System.Drawing.Font("Microsoft Sans Serif", 9f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TextBox1.Location = new System.Drawing.Point(799, 12);
		this.TextBox1.Name = "TextBox1";
		this.TextBox1.Size = new System.Drawing.Size(230, 24);
		this.TextBox1.TabIndex = 0;
		this.TextBox1.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.ListBox1.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left | System.Windows.Forms.AnchorStyles.Right;
		this.ListBox1.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ListBox1.FormattingEnabled = true;
		this.ListBox1.Location = new System.Drawing.Point(12, 85);
		this.ListBox1.Name = "ListBox1";
		this.ListBox1.Size = new System.Drawing.Size(1017, 378);
		this.ListBox1.TabIndex = 1;
		this.OkB.Anchor = System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Right;
		this.OkB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkB.Location = new System.Drawing.Point(862, 469);
		this.OkB.Name = "OkB";
		this.OkB.Size = new System.Drawing.Size(167, 40);
		this.OkB.TabIndex = 7;
		this.OkB.Text = "Вибрати";
		this.OkB.UseVisualStyleBackColor = true;
		this.NoB.Anchor = System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left;
		this.NoB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoB.Location = new System.Drawing.Point(12, 469);
		this.NoB.Name = "NoB";
		this.NoB.Size = new System.Drawing.Size(167, 40);
		this.NoB.TabIndex = 8;
		this.NoB.Text = "Скасувати";
		this.NoB.UseVisualStyleBackColor = true;
		this.SeaT.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SeaT.Location = new System.Drawing.Point(12, 32);
		this.SeaT.Name = "SeaT";
		this.SeaT.Size = new System.Drawing.Size(414, 28);
		this.SeaT.TabIndex = 9;
		this.SeaT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.SeaB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SeaB.Location = new System.Drawing.Point(447, 20);
		this.SeaB.Name = "SeaB";
		this.SeaB.Size = new System.Drawing.Size(167, 40);
		this.SeaB.TabIndex = 10;
		this.SeaB.Text = "Пошук ";
		this.SeaB.UseVisualStyleBackColor = true;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1042, 525);
		base.Controls.Add(this.SeaB);
		base.Controls.Add(this.SeaT);
		base.Controls.Add(this.NoB);
		base.Controls.Add(this.OkB);
		base.Controls.Add(this.ListBox1);
		base.Controls.Add(this.TextBox1);
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormImport";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Iмпорт даних ";
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormImport(string jsonText)
	{
		base.Load += FormImport_Load;
		InfaC = 0;
		Infa = new TypNumFiscal[checked(InfaC + 1)];
		JS = "";
		InitializeComponent();
		JS = jsonText;
	}

	private void FormImport_Load(object sender, EventArgs e)
	{
		All.InfaImport.IPN = "";
		All.InfaImport.Name = "";
		All.InfaImport.Address = "";
		All.InfaImport.NumFiscal = "";
		All.InfaImport.TIN = "";
		All.InfaImport.OrgName = "";
		Show();
		Application.DoEvents();
		Dereban();
	}

	private bool Dereban()
	{
		ListBox1.Items.Clear();
		SeaB.Enabled = false;
		OkB.Enabled = false;
		NoB.Enabled = false;
		checked
		{
			bool result;
			try
			{
				string[] array = JS.Split('{');
				int num = array.Length - 1;
				for (int i = 0; i <= num; i++)
				{
					array[i] = Strings.Replace(array[i], "{", "");
					array[i] = Strings.Replace(array[i], "}", "");
					array[i] = Strings.Replace(array[i], "[", "\"WebCheck\"");
					array[i] = Strings.Replace(array[i], "],", "H*G*B");
					array[i] = Strings.Replace(array[i], "]", "");
				}
				TypEntity typEntity = default(TypEntity);
				typEntity.TIN = "";
				typEntity.OrgName = "";
				typEntity.Address = "";
				typEntity.IPN = "";
				typEntity.NameG = "";
				int num2 = 0;
				Infa = new TypNumFiscal[num2 + 1];
				int num3 = array.Length - 1;
				for (int i = 2; i <= num3; i++)
				{
					if (Operators.CompareString(KeyJ(array[i], "Entity"), "", TextCompare: false) != 0)
					{
						typEntity.OrgName = KeyJ(array[i], "OrgName");
						typEntity.Address = KeyJ(array[i], "Address");
						typEntity.IPN = KeyJ(array[i], "Ipn");
						typEntity.TIN = KeyJ(array[i], "Tin");
						typEntity.NameG = KeyJ(array[i], "Name");
					}
					else if (Operators.CompareString(KeyJ(array[i], "Closed", fn: true).ToLower(), "false", TextCompare: false) == 0)
					{
						Infa[num2].OrgName = typEntity.OrgName;
						Infa[num2].Address = typEntity.Address;
						Infa[num2].IPN = typEntity.IPN;
						Infa[num2].TIN = typEntity.TIN;
						Infa[num2].NumFiscal = KeyJ(array[i], "NumFiscal", fn: true);
						Infa[num2].Name = typEntity.NameG;
						ListBox1.Items.Add(Infa[num2].NumFiscal + "     " + Infa[num2].Address);
						num2++;
						ref TypNumFiscal[] infa = ref Infa;
						infa = (TypNumFiscal[])Utils.CopyArray(infa, new TypNumFiscal[num2 + 1]);
					}
					TextBox1.Text = (array.Length - i - 1).ToString();
					Application.DoEvents();
				}
				TextBox1.Text = num2.ToString();
				SeaB.Enabled = true;
				OkB.Enabled = true;
				NoB.Enabled = true;
				result = true;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				TextBox1.Text = "Виникла помилка";
				SeaB.Enabled = true;
				OkB.Enabled = true;
				NoB.Enabled = true;
				result = false;
				ProjectData.ClearProjectError();
			}
			return result;
		}
	}

	private string KeyJ(string strJS, string Key, bool fn = false)
	{
		string result;
		try
		{
			if (!fn)
			{
				strJS = "{" + strJS.Trim() + "}";
			}
			else
			{
				strJS = "{" + strJS.Replace("e,", "e").Trim() + "}";
				strJS = Strings.Replace(strJS, "H*G*B", ",");
			}
			result = ((IDictionary<string, object>)JsonConvert.DeserializeObject<ExpandoObject>(strJS))[Key.Trim()].ToString();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private void ListBox1_SelectedIndexChanged(object sender, EventArgs e)
	{
		checked
		{
			if (ListBox1.SelectedIndex >= 0)
			{
				int num = ListBox1.Items.Count - 1;
				for (int i = 0; i <= num; i++)
				{
					ListBox1.SetItemChecked(i, value: false);
				}
				ListBox1.SetItemChecked(ListBox1.SelectedIndex, value: true);
			}
		}
	}

	private void SeaB_Click(object sender, EventArgs e)
	{
		SeaB.Enabled = false;
		OkB.Enabled = false;
		NoB.Enabled = false;
		if (Operators.CompareString(SeaT.Text.Trim(), "", TextCompare: false) == 0)
		{
			SeashAll();
		}
		else
		{
			Seash();
		}
		SeaB.Enabled = true;
		OkB.Enabled = true;
		NoB.Enabled = true;
	}

	private void Seash()
	{
		ListBox1.Items.Clear();
		InfaC = 0;
		string value = SeaT.Text.Trim();
		checked
		{
			int num = Infa.Length - 2;
			for (int i = 0; i <= num; i++)
			{
				if (Infa[i].NumFiscal.IndexOf(value) > -1)
				{
					ListBox1.Items.Add(Infa[i].NumFiscal + "     " + Infa[i].Address);
					InfaC++;
				}
				else if (Infa[i].Address.IndexOf(value) > -1)
				{
					ListBox1.Items.Add(Infa[i].NumFiscal + "     " + Infa[i].Address);
					InfaC++;
				}
			}
			TextBox1.Text = InfaC.ToString();
		}
	}

	private void SeashAll()
	{
		ListBox1.Items.Clear();
		InfaC = 0;
		SeaT.Text.Trim();
		checked
		{
			int num = Infa.Length - 2;
			for (int i = 0; i <= num; i++)
			{
				ListBox1.Items.Add(Infa[i].NumFiscal + "     " + Infa[i].Address);
				InfaC++;
			}
			TextBox1.Text = InfaC.ToString();
		}
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		int selectedIndex = ListBox1.SelectedIndex;
		if (selectedIndex > -1 && SeashImport(ListBox1.Items[selectedIndex].ToString()))
		{
			Close();
		}
	}

	private bool SeashImport(string sT)
	{
		sT = sT.Trim();
		string right = Conversions.ToString(sT[0]) + Conversions.ToString(sT[1]) + Conversions.ToString(sT[2]) + Conversions.ToString(sT[3]) + Conversions.ToString(sT[4]) + Conversions.ToString(sT[5]) + Conversions.ToString(sT[6]) + Conversions.ToString(sT[7]) + Conversions.ToString(sT[8]) + Conversions.ToString(sT[9]);
		checked
		{
			int num = Infa.Length - 2;
			for (int i = 0; i <= num; i++)
			{
				if (Operators.CompareString(Infa[i].NumFiscal, right, TextCompare: false) == 0)
				{
					All.InfaImport.IPN = Infa[i].IPN;
					All.InfaImport.Name = Infa[i].Name;
					All.InfaImport.Address = Infa[i].Address;
					All.InfaImport.NumFiscal = Infa[i].NumFiscal;
					All.InfaImport.TIN = Infa[i].TIN;
					All.InfaImport.OrgName = Infa[i].OrgName;
					return true;
				}
			}
			return false;
		}
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		All.InfaImport.IPN = "";
		All.InfaImport.Name = "";
		All.InfaImport.Address = "";
		All.InfaImport.NumFiscal = "";
		All.InfaImport.TIN = "";
		All.InfaImport.OrgName = "";
		Close();
	}
}
